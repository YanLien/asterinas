// SPDX-License-Identifier: MPL-2.0

#include <errno.h>
#include <sys/syscall.h>
#include <unistd.h>
#include "../../common/test.h"

#ifndef SYS_getrandom
#if defined(__x86_64__)
#define SYS_getrandom 318
#elif defined(__aarch64__)
#define SYS_getrandom 278
#elif defined(__riscv)
#define SYS_getrandom 278
#else
#define SYS_getrandom 278
#endif
#endif

#define GRND_NONBLOCK 0x0001
#define GRND_RANDOM 0x0002
#define GRND_INSECURE 0x0004

static ssize_t getrandom(void *buf, size_t count, unsigned int flags)
{
	return syscall(SYS_getrandom, buf, count, flags);
}

FN_TEST(getrandom_default)
{
	char buf[64];
	// Basic call with no flags.
	TEST_RES(getrandom(buf, sizeof(buf), 0), _ret == sizeof(buf));
}
END_TEST()

FN_TEST(getrandom_zero_count)
{
	char buf[1];
	// Zero count should return 0.
	TEST_RES(getrandom(buf, 0, 0), _ret == 0);
}
END_TEST()

FN_TEST(getrandom_nonblock)
{
	char buf[64];
	// GRND_NONBLOCK should succeed since the RNG is always initialized.
	TEST_RES(getrandom(buf, sizeof(buf), GRND_NONBLOCK), _ret == sizeof(buf));
}
END_TEST()

FN_TEST(getrandom_insecure)
{
	char buf[64];
	// GRND_INSECURE should behave like the default path.
	TEST_RES(getrandom(buf, sizeof(buf), GRND_INSECURE), _ret == sizeof(buf));
}
END_TEST()

FN_TEST(getrandom_nonblock_insecure)
{
	char buf[64];
	// Combining GRND_NONBLOCK | GRND_INSECURE should work.
	TEST_RES(getrandom(buf, sizeof(buf), GRND_NONBLOCK | GRND_INSECURE),
		 _ret == sizeof(buf));
}
END_TEST()

FN_TEST(getrandom_grnd_random)
{
	char buf[64];
	// GRND_RANDOM should also work (same underlying RNG).
	TEST_RES(getrandom(buf, sizeof(buf), GRND_RANDOM), _ret == sizeof(buf));
}
END_TEST()

FN_TEST(getrandom_insecure_random_rejected)
{
	char buf[64];
	// GRND_INSECURE | GRND_RANDOM is invalid.
	TEST_ERRNO(getrandom(buf, sizeof(buf), GRND_INSECURE | GRND_RANDOM), EINVAL);
}
END_TEST()

FN_TEST(getrandom_invalid_flags)
{
	char buf[64];
	// Undefined flag bits should be rejected.
	TEST_ERRNO(getrandom(buf, sizeof(buf), 0x0008), EINVAL);
}
END_TEST()

FN_TEST(getrandom_fault)
{
	// NULL buffer with non-zero count should fault.
	TEST_ERRNO(getrandom(NULL, 64, 0), EFAULT);
}
END_TEST()

FN_TEST(getrandom_produces_bytes)
{
	char buf[64];
	memset(buf, 0, sizeof(buf));
	TEST_RES(getrandom(buf, sizeof(buf), 0), _ret == sizeof(buf));

	// Extremely unlikely that all bytes are zero.
	int all_zero = 1;
	for (size_t i = 0; i < sizeof(buf); i++) {
		if (buf[i] != 0) {
			all_zero = 0;
			break;
		}
	}
	TEST_RES(1, !all_zero);
}
END_TEST()
