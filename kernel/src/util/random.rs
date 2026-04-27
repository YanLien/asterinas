// SPDX-License-Identifier: MPL-2.0

use core::{
    cell::Cell,
    sync::atomic::{AtomicBool, Ordering},
};

use jhash;
use rand::{RngCore, SeedableRng, rngs::StdRng};
use spin::Once;

use crate::prelude::*;

/// Pool size in u32 words (8 × 4 = 32 bytes, matching `StdRng` seed size).
const POOL_SIZE: usize = 8;

/// Reseed every 300 seconds (300 × 1000 Hz).
const RESEED_INTERVAL_JIFFIES: u64 = 300_000;

/// Perform the first reseed after this many samples (~64 ms at 1000 Hz).
const INITIAL_RESEED_THRESHOLD: u32 = 64;

static RNG: Once<SpinLock<StdRng>> = Once::new();
static ENTROPY_POOL: Once<SpinLock<EntropyPool, LocalIrqDisabled>> = Once::new();

/// Flag set by the timer callback when a reseed is due.
static RESEED_NEEDED: AtomicBool = AtomicBool::new(false);

/// Per-CPU last TSC value for computing jitter deltas.
ostd::cpu_local! {
    static LAST_TSC: Cell<u64> = Cell::new(0);
}

struct EntropyPool {
    pool: [u32; POOL_SIZE],
    cursor: usize,
    sample_count: u32,
    last_reseed_jiffies: u64,
}

impl EntropyPool {
    fn new() -> Self {
        Self {
            pool: [0u32; POOL_SIZE],
            cursor: 0,
            sample_count: 0,
            last_reseed_jiffies: 0,
        }
    }

    fn add_sample(&mut self, sample: u32) {
        self.pool[self.cursor] ^= sample;
        self.pool[self.cursor] =
            jhash::jhash_u32_array(&self.pool, self.cursor as u32);
        self.cursor = (self.cursor + 1) % POOL_SIZE;
        self.sample_count += 1;
    }

    fn extract_seed(&self) -> <StdRng as SeedableRng>::Seed {
        let mut seed = <StdRng as SeedableRng>::Seed::default();
        let seed_bytes = seed.as_mut();

        // Convert pool words to bytes.
        let mut raw = [0u8; 32];
        for (i, &word) in self.pool.iter().enumerate() {
            raw[i * 4..(i + 1) * 4].copy_from_slice(&word.to_le_bytes());
        }

        // Fold with a jhash-derived key to avoid exposing raw pool state.
        let key = jhash::jhash_u32_array(&self.pool, 0x4e647261);
        for (i, byte) in seed_bytes.iter_mut().enumerate() {
            *byte = raw[i] ^ (key.wrapping_add(i as u32) as u8);
        }

        seed
    }

    fn should_reseed(&self, current_jiffies: u64) -> bool {
        if self.last_reseed_jiffies == 0 && self.sample_count >= INITIAL_RESEED_THRESHOLD {
            return true;
        }
        current_jiffies.wrapping_sub(self.last_reseed_jiffies) >= RESEED_INTERVAL_JIFFIES
    }
}

/// Timer IRQ callback that collects environmental noise.
fn collect_entropy_on_timer() {
    let tsc = ostd::arch::read_tsc();

    // Update per-CPU last TSC.
    let irq_guard = ostd::irq::disable_local();
    let last_tsc_cell = LAST_TSC.get_with(&irq_guard);
    let last_tsc = last_tsc_cell.get();
    last_tsc_cell.set(tsc);
    drop(irq_guard);

    // Skip the first tick (no reference point).
    if last_tsc == 0 {
        return;
    }

    // Fold 64-bit TSC delta into a 32-bit sample.
    let delta = tsc.wrapping_sub(last_tsc);
    let jitter_sample = jhash::jhash_2vals(delta as u32, (delta >> 32) as u32, 0);

    // Optionally mix hardware RNG.
    let sample = match ostd::arch::read_random() {
        Some(hw_rand) => jhash::jhash_3vals(
            jitter_sample,
            hw_rand as u32,
            (hw_rand >> 32) as u32,
            1,
        ),
        None => jitter_sample,
    };

    let reseed_needed = {
        let pool = ENTROPY_POOL.get().unwrap();
        let mut pool = pool.lock();
        pool.add_sample(sample);

        let current_jiffies = ostd::timer::Jiffies::elapsed().as_u64();
        let should = pool.should_reseed(current_jiffies);
        if should {
            pool.last_reseed_jiffies = current_jiffies;
        }
        should
    };

    if reseed_needed {
        RESEED_NEEDED.store(true, Ordering::Relaxed);
    }
}

/// Fill `dest` with random bytes.
///
/// The underlying CSPRNG is periodically reseeded from an entropy pool
/// that collects TSC jitter and hardware RNG noise.
pub fn getrandom(dst: &mut [u8]) {
    maybe_reseed();
    RNG.get().unwrap().lock().fill_bytes(dst);
}

fn maybe_reseed() {
    if !RESEED_NEEDED.load(Ordering::Relaxed) {
        return;
    }

    // Clear the flag; if another reseed becomes needed during extraction,
    // the timer callback will set it again.
    RESEED_NEEDED.store(false, Ordering::Relaxed);

    let pool = ENTROPY_POOL.get().unwrap();
    let seed = pool.disable_irq().lock().extract_seed();

    // XOR pool seed with current RNG output for forward secrecy.
    let new_seed = {
        let mut rng = RNG.get().unwrap().lock();
        let mut current_output = <StdRng as SeedableRng>::Seed::default();
        rng.fill_bytes(current_output.as_mut());
        let mut combined = seed;
        for (i, byte) in combined.as_mut().iter_mut().enumerate() {
            *byte ^= current_output[i];
        }
        combined
    };

    *RNG.get().unwrap().lock() = StdRng::from_seed(new_seed);
}

pub fn init() {
    let seed = get_random_seed();
    ENTROPY_POOL.call_once(|| SpinLock::new(EntropyPool::new()));
    RNG.call_once(|| SpinLock::new(StdRng::from_seed(seed)));
}

/// Registers the timer callback for entropy collection on the current CPU.
///
/// Must be called once per CPU, after the timer infrastructure is initialized.
pub fn init_on_each_cpu() {
    ostd::timer::register_callback_on_cpu(collect_entropy_on_timer);
}

#[cfg(target_arch = "x86_64")]
fn get_random_seed() -> <StdRng as SeedableRng>::Seed {
    use ostd::arch::read_random;

    let mut seed = <StdRng as SeedableRng>::Seed::default();

    // Notes for future refactorings: If hardware randomness cannot be generated (i.e., if
    // `read_random` fails), we can usually continue with pseudorandomness. However, we should stop
    // if we are TD guests. For more details, see
    // <https://intel.github.io/ccc-linux-guest-hardening-docs/security-spec.html#randomness-inside-tdx-guest>.
    let mut chunks = seed.as_mut().chunks_exact_mut(size_of::<u64>());
    for chunk in chunks.by_ref() {
        let src = read_random().expect("`read_random` failed").to_ne_bytes();
        chunk.copy_from_slice(&src);
    }
    let tail = chunks.into_remainder();
    let n = tail.len();
    if n > 0 {
        let src = read_random().expect("`read_random` failed").to_ne_bytes();
        tail.copy_from_slice(&src[..n]);
    }

    seed
}

#[cfg(not(target_arch = "x86_64"))]
fn get_random_seed() -> <StdRng as SeedableRng>::Seed {
    use ostd::arch::boot::DEVICE_TREE;

    let chosen = DEVICE_TREE.get().unwrap().find_node("/chosen").unwrap();
    chosen
        .property("rng-seed")
        .unwrap()
        .value
        .try_into()
        .unwrap()
}
