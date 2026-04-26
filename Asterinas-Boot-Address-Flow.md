# Asterinas x86_64 启动地址跳转说明

本文档描述 Asterinas 在 x86_64 上启动时的地址变化过程，重点解释
CPU 从 bootloader 交给内核开始，到进入 `kernel/src/init.rs` 之前，
地址如何从物理地址、临时页表映射、高半区虚拟地址，最终切换到 OSTD
正式内核页表。

本文只覆盖 x86_64。RISC-V 和 LoongArch 的链接地址、启动协议和映射策略不同。

本文以这条命令为主线：

```bash
CONSOLE=ttyS0 make run_kernel LOG_LEVEL=debug BOOT_PROTOCOL=linux-efi-handover64
```

这条命令的启动链路是：

```text
QEMU
  -> OVMF/UEFI
  -> GRUB rescue ISO
  -> GRUB `linux` 命令
  -> OSDK 生成的 EFI64 bzImage
  -> Linux x86 Boot Protocol 的 EFI handover64 入口
  -> Asterinas ELF 中的 __linux64_boot
  -> __linux_boot
  -> OSTD / kernel 初始化
```

其中：

```text
CONSOLE=ttyS0
  -> QEMU 串口接到当前终端
  -> kernel cmdline 里加入 console=ttyS0

LOG_LEVEL=debug
  -> kernel cmdline 里加入 ostd.log_level=debug

BOOT_PROTOCOL=linux-efi-handover64
  -> OSDK 生成 Linux bzImage 兼容镜像
  -> GRUB 配置使用 linux/initrd 命令
  -> EFI handover64 入口最终跳入 Asterinas 的 Linux 64-bit boot 入口
```

## 1. 总览

x86_64 启动阶段可以粗略分成四层：

```text
QEMU / OVMF / GRUB
        |
        v
Linux EFI handover stub
  OSDK 生成的 bzImage setup 代码
        |
        v
低地址 boot 代码
  Asterinas ELF 的 .bsp_boot / .ap_boot
        |
        v
临时 boot page table
  identity mapping + high-half kernel mapping + early linear mapping
        |
        v
高半区 Rust 入口
  本命令走 __linux_boot
  Multiboot/Multiboot2 是其他可选路径
        |
        v
OSTD 正式初始化
  crate::init()
  init_kernel_page_table()
  activate_kernel_page_table()
        |
        v
kernel crate 入口
  #[ostd::main] -> kernel::init::main()
```

最关键的地址关系是：

```text
KERNEL_LMA = 0x0800_0000
KERNEL_VMA = 0xffff_ffff_8000_0000

普通内核段虚拟地址 = 普通内核段物理地址 + KERNEL_VMA
```

例如：

```text
物理地址 0x0800_0000
  -> 高半区虚拟地址 0xffff_ffff_8800_0000
```

注意：`KERNEL_VMA` 是内核代码的高半区偏移，不是物理内存 linear mapping
的基址。正式运行后，物理内存 direct map 使用的是
`0xffff_8000_0000_0000 + PA`。

## 2. 链接脚本中的地址布局

x86_64 链接脚本模板在：

```text
osdk/src/base_crate/x86_64.ld.template
```

核心常量：

```ld
KERNEL_LMA = 0x8000000;
BSP_BOOT_LMA = 0x8001000;
AP_EXEC_MA = 0x8000;
KERNEL_VMA = 0xffffffff80000000;
```

含义如下：

| 名称 | 值 | 含义 |
| --- | --- | --- |
| `KERNEL_LMA` | `0x0800_0000` | 内核 ELF 期望的物理加载基址 |
| `BSP_BOOT_LMA` | `0x0800_1000` | BSP boot 入口所在物理地址 |
| `AP_EXEC_MA` | `0x0000_8000` | AP 启动代码最终执行的低物理地址 |
| `KERNEL_VMA` | `0xffff_ffff_8000_0000` | 普通内核段的高半区虚拟偏移 |

链接后的主要 section 顺序：

```text
0x0800_0000 + KERNEL_VMA
    .multiboot_header
    .multiboot2_header

0x0800_1000
    .bsp_boot
    .bsp_boot.stack

0x0000_8000
    .ap_boot
    注意：链接时 VMA 是低地址，但实际 load address 在内核镜像中。
    BSP 后续会把它复制到物理 0x8000。

高半区虚拟地址
    .text
    .ex_table
    .ktest_array
    .init_array
    .sensitive_io_ports
    .rodata
    .eh_frame*
    .data
    .cpu_local_tss
    .cpu_local
    .bss
```

普通内核段使用：

```ld
AT(ADDR(section) - KERNEL_VMA)
```

所以普通段的 VMA 在高半区，但 LMA 是低物理地址。

注意：`BOOT_PROTOCOL=linux-efi-handover64` 时，GRUB 直接加载的不是这个
ELF 文件本身，而是 OSDK 用它包装出来的 EFI64 bzImage。bzImage 中的 setup
代码会把真正的 Asterinas ELF payload 加载到上述 ELF 期望的位置，然后跳入
ELF 中的 Linux 64-bit boot 入口。

## 3. BSP 入口：bootloader 跳到低地址 boot 段

BSP 启动代码在：

```text
ostd/src/arch/x86/boot/bsp_boot.S
```

根据启动协议不同，入口不同：

```text
Linux 32-bit Boot Protocol -> __linux32_boot
Linux 64-bit Boot Protocol -> __linux64_boot
Multiboot / Multiboot2     -> __multiboot_boot
```

这些入口都在 `.bsp_boot` 段中，链接到 `BSP_BOOT_LMA = 0x0800_1000`
附近。此时 CPU 看到的是低物理地址，尚未进入 Asterinas 的正式高半区
运行环境。

本文对应的命令使用的是 Linux EFI handover64 路径。它不是直接让 GRUB 跳到
`__linux64_boot`。实际过程是：

```text
GRUB `linux`
  -> bzImage header 中声明支持 EFI handover64
  -> GRUB/UEFI 调用 bzImage setup 的 entry_efi_handover64
  -> setup 代码整理 boot_params、cmdline、initrd、E820 memory map
  -> setup 代码跳到 Asterinas ELF 的 0x0800_1200
  -> 0x0800_1200 对应 __linux64_boot
```

`0x0800_1200` 来自：

```text
BSP_BOOT_LMA = 0x0800_1000
__linux64_boot 在 .bsp_boot 内偏移 0x200
```

OSDK 的 EFI64 bzImage setup 中也写死了这个 Asterinas 入口：

```rust
const ASTER_ENTRY_POINT: *const () = 0x8001200 as _;
```

Multiboot/Multiboot2 路径进入时，bootloader 提供：

```text
eax = magic
ebx = multiboot info physical address
```

汇编代码把这些值压入 boot 栈，同时压入一个内部入口类型：

```text
ENTRYTYPE_MULTIBOOT
ENTRYTYPE_MULTIBOOT2
ENTRYTYPE_LINUX_32
ENTRYTYPE_LINUX_64
```

这样后面统一进入 `long_mode` 后，可以根据栈上的类型跳到对应 Rust
入口。

对于本文的 Linux EFI handover64 路径，进入 `__linux64_boot` 时，
setup 代码把 `boot_params` 指针放在 `rsi` 中。`__linux64_boot` 会把它压到
栈上，并把内部入口类型标记为：

```text
ENTRYTYPE_LINUX_64
```

## 4. 临时 boot page table

在进入 long mode 之前，Asterinas 自己构建一套临时页表：

```text
boot_l4pt
boot_l3pt_linear_id
boot_l3pt_kernel
boot_l2pt_0g_1g
boot_l2pt_1g_2g
boot_l2pt_2g_3g
boot_l2pt_3g_4g
```

这些页表也放在 `.bsp_boot` 段里。

临时页表建立三类重要映射。

第一类是低 4 GiB identity mapping：

```text
VA 0x0000_0000_0000_0000 .. 0x0000_0000_ffff_ffff
PA 0x0000_0000_0000_0000 .. 0x0000_0000_ffff_ffff
```

这让 CPU 在刚开启分页后，仍然可以继续执行低地址处的 boot 代码。

第二类是早期 linear mapping：

```text
VA 0xffff_8000_0000_0000 .. 0xffff_8000_ffff_ffff
PA 0x0000_0000_0000_0000 .. 0x0000_0000_ffff_ffff
```

早期 Rust 代码解析 bootloader 传来的物理地址时会用到这个映射。例如
Multiboot2 的 command line、initramfs 地址，本质是 bootloader 给出的
物理地址，代码会用 `paddr_to_vaddr()` 转成可访问的高半区地址。

第三类是内核代码高半区映射：

```text
VA 0xffff_ffff_8000_0000 .. 0xffff_ffff_ffff_ffff
PA 0x0000_0000_0000_0000 .. 0x0000_0000_7fff_ffff
```

这让 `.text`、`.rodata`、`.data`、`.bss` 等普通内核段可以按链接时的
高半区地址运行。

临时页表使用 2 MiB huge page 映射低 4 GiB 物理内存。

## 5. 从低地址跳到高半区

Linux EFI handover64 路径在进入 Asterinas ELF 前，已经先运行了一段 OSDK
生成的 bzImage setup 代码。这段代码在 UEFI 环境中处理 cmdline、initrd、
ACPI、framebuffer 和 EFI memory map，然后退出 EFI boot services，并跳入：

```text
0x0800_1200 = __linux64_boot
```

进入 `__linux64_boot` 后，流程是：

```text
entry_efi_handover64
        |
        v
bzImage setup / EFI stub
        |
        | 整理 boot_params
        | 退出 EFI boot services
        v
__linux64_boot
        |
        | rsi = boot_params ptr
        | 设置 boot stack
        | 压入 ENTRYTYPE_LINUX_64
        | 建立 Asterinas 临时页表
        | 设置 CR3
        | 切到 Asterinas GDT
        v
long_mode_in_low_address
```

Multiboot 和 Linux 32-bit 是其他可选路径，大致流程如下：

```text
__multiboot_boot / __linux32_boot
        |
        v
initial_boot_setup
        |
        v
protected_mode
        |
        | 设置临时页表
        | 开启 PAE / PGE
        | 设置 CR3
        | 开启 EFER.LME
        | 开启 CR0.PG
        v
long_mode_in_low_address
```

Linux 64-bit 路径已经由 EFI handover setup 进入 64 位环境，但仍会设置
Asterinas 自己的临时页表，然后进入同一个 `long_mode_in_low_address`。

`long_mode_in_low_address` 是最关键的地址跳动点：

```asm
mov rbx, KERNEL_VMA
or  rsp, rbx
mov rax, offset long_mode
jmp rax
```

这里做了两件事：

1. 把栈指针 `rsp` 加到高半区。
2. 跳转到高半区的 `long_mode`。

这里使用 `or rsp, KERNEL_VMA`，是因为 boot 栈原本位于低地址，且
`KERNEL_VMA` 的低 31 位为 0；效果等价于把低地址栈映射到
`0xffff_ffff_8000_0000` 这一高半区窗口。

跳转前：

```text
RIP = long_mode_in_low_address 的低地址映射
RSP = boot_stack_top 的低地址映射
```

跳转后：

```text
RIP = long_mode 的高半区虚拟地址
RSP = boot_stack_top + KERNEL_VMA
```

从 `long_mode` 开始，代码注释明确写着：

```text
From here, we're in the .text section: we no longer use physical address.
```

## 6. 高半区汇编进入 Rust

进入 `long_mode` 后，汇编会：

1. 清空 `.bss`。
2. 清空 `rbp`，方便栈回溯终止。
3. 设置 `GS.base = __cpu_local_start`。
4. 从栈上取出入口类型。
5. 调用对应 Rust 入口：

```text
ENTRYTYPE_LINUX_*     -> __linux_boot
ENTRYTYPE_MULTIBOOT   -> __multiboot_entry
ENTRYTYPE_MULTIBOOT2  -> __multiboot2_entry
```

本文命令走的是：

```text
ENTRYTYPE_LINUX_64 -> __linux_boot
```

这些 Rust 入口已经运行在高半区虚拟地址下。

## 7. Rust boot 入口如何处理物理地址

本文命令走 Linux boot protocol，对应 Rust 入口在：

```text
ostd/src/arch/x86/boot/linux_boot/mod.rs
```

`__linux_boot` 收到的是一个 `BootParams` 指针。这个结构来自 Linux x86 Boot
Protocol，里面包含：

```text
cmd_line_ptr / cmdline_size
ramdisk_image / ramdisk_size
e820_table / e820_entries
acpi_rsdp_addr
screen_info
```

这些字段里的 cmdline、initrd 等地址仍然按物理地址理解。Asterinas 会通过：

```rust
paddr_to_vaddr(pa)
```

转成高半区地址再访问。

在早期阶段，`paddr_to_vaddr()` 使用的是 OSTD kernel space 中定义的
linear mapping 规则：

```rust
va = pa + LINEAR_MAPPING_BASE_VADDR
```

x86_64 下：

```text
LINEAR_MAPPING_BASE_VADDR = 0xffff_8000_0000_0000
```

所以一个 bootloader 给出的物理地址 `0x1234_0000` 会被访问为：

```text
0xffff_8000_1234_0000
```

这正是前面临时 boot page table 建立的早期 linear mapping 的用途。

Multiboot2 路径也类似，只是 Rust 入口换成：

```text
ostd/src/arch/x86/boot/multiboot2/mod.rs
```

它解析的是 Multiboot2 information structure，而不是 Linux `BootParams`。

Rust boot 入口会填充：

```text
EARLY_INFO
```

其中包括：

```text
bootloader_name
kernel_cmdline
initramfs
acpi_arg
framebuffer_arg
memory_regions
```

然后调用：

```rust
start_kernel()
```

## 8. OSTD 初始化期间的地址阶段

`start_kernel()` 会先调用：

```rust
crate::init()
```

这个函数在：

```text
ostd/src/lib.rs
```

其中和地址切换密切相关的步骤是：

```text
init_early_allocator()
serial::init()
log::init()
cpu::init_on_bsp()
frame metadata init
frame allocator init
init_kernel_page_table()
activate_kernel_page_table()
sync::init()
boot::init_after_heap()
arch::late_init_on_bsp()
smp::init()
boot_pt::dismiss()
```

在 `activate_kernel_page_table()` 之前，CPU 仍然使用 boot 阶段页表。这个
页表最初来自 `.bsp_boot`，之后可能被 `BootPageTable` 包装和扩展。

`BootPageTable` 的作用是：在正式页表还没有建立完成前，允许早期代码
临时管理当前页表，同时记录哪些页表页后面可以释放。

## 9. OSTD 正式内核虚拟地址空间

正式内核地址空间定义在：

```text
ostd/src/mm/kspace/mod.rs
```

48-bit x86_64 下布局如下：

```text
0xffff_ffff_ffff_0000  KERNEL_END_VADDR
        ^
        |  kernel code area, about 1 GiB
0xffff_ffff_8000_0000  KERNEL_CODE_BASE_VADDR

        |  unused hole

0xffff_e100_0000_0000
        |  frame metadata, 1 TiB
0xffff_e000_0000_0000  FRAME_METADATA_BASE_VADDR

        |  KVirtArea / vmalloc, 32 TiB
0xffff_c000_0000_0000  VMALLOC_BASE_VADDR

        |  linear mapping, 64 TiB
        |  VA = PA + 0xffff_8000_0000_0000
0xffff_8000_0000_0000  LINEAR_MAPPING_BASE_VADDR
```

相关常量：

```text
KERNEL_BASE_VADDR          = 0xffff_8000_0000_0000
LINEAR_MAPPING_BASE_VADDR  = 0xffff_8000_0000_0000
VMALLOC_BASE_VADDR         = 0xffff_c000_0000_0000
FRAME_METADATA_BASE_VADDR  = 0xffff_e000_0000_0000
KERNEL_CODE_BASE_VADDR     = 0xffff_ffff_8000_0000
KERNEL_END_VADDR           = 0xffff_ffff_ffff_0000
```

注意这里有两个不同的高半区映射概念：

| 映射 | 公式 | 用途 |
| --- | --- | --- |
| kernel code mapping | `VA = PA + 0xffff_ffff_8000_0000` | 执行内核 `.text/.data/.bss` |
| linear mapping | `VA = PA + 0xffff_8000_0000_0000` | 通过物理地址访问任意物理内存 |

## 10. 正式 kernel page table 建立

正式页表由：

```rust
init_kernel_page_table()
```

建立。它主要创建三类映射。

第一类：完整 physical memory linear mapping。

```text
VA = LINEAR_MAPPING_BASE_VADDR + PA
```

范围从：

```text
LINEAR_MAPPING_BASE_VADDR
```

到：

```text
LINEAR_MAPPING_BASE_VADDR + max_paddr
```

第二类：frame metadata 映射。

frame metadata 是每个物理页帧对应的元数据区域，用于 OSTD 的 frame
管理。它被映射到 `FRAME_METADATA_BASE_VADDR` 附近。

第三类：kernel code mapping。

代码会从 boot memory regions 中找到 `MemoryRegionType::Kernel`：

```rust
let from = region.base() + kernel_loaded_offset()
        .. region.end() + kernel_loaded_offset();
```

x86_64 下：

```text
kernel_loaded_offset() = KERNEL_CODE_BASE_VADDR
                       = 0xffff_ffff_8000_0000
```

所以正式页表继续保持：

```text
kernel physical region
  -> physical + 0xffff_ffff_8000_0000
```

当前实现对 kernel code mapping 使用 `RWX`：

```text
TODO: set separated permissions for each segments in the kernel.
```

也就是说，虽然 ELF 里 `.text/.rodata/.data` 有不同 segment flag，
正式页表目前还没有按段拆分权限。

## 11. 切换到正式页表

正式页表建立后：

```rust
activate_kernel_page_table()
```

会把 CPU 的页表根切到 OSTD 新建的 kernel page table。

这次切换之后：

```text
旧 boot page table 不再是当前页表
正式 kernel page table 接管地址翻译
```

但是关键虚拟地址都保持有效：

```text
当前执行的 RIP 仍然在 kernel code mapping 中
当前 RSP 仍然在 kernel code mapping 或后续内核栈映射中
paddr_to_vaddr() 仍然指向 linear mapping
```

因此这次页表切换不是一次显式的代码跳转，而是一次“同一虚拟地址在新页表
下继续有效”的切换。

随后：

```rust
boot_pt::dismiss()
```

会释放或丢弃 boot 阶段页表资源。此后不应再使用 boot page table。

## 12. 进入 kernel crate

OSTD 初始化完成后，`start_kernel()` 调用：

```rust
__ostd_main()
```

这个符号由 `#[ostd::main]` 宏生成。Asterinas kernel crate 中对应代码：

```rust
#[ostd::main]
fn main() {
    init::main();
}
```

也就是最终进入：

```text
kernel/src/init.rs
```

中的：

```rust
pub(super) fn main()
```

此时已经处于：

```text
高半区虚拟地址
正式 OSTD kernel page table
BSP boot context
```

所以这行：

```rust
ostd::early_println!("OSTD initialized. Preparing components.");
```

已经不是最早启动输出，而是 OSTD 完成基础初始化、即将初始化 kernel
components 时的输出。

## 13. AP 启动地址跳转

AP 启动和 BSP 不完全一样。

链接脚本里 `.ap_boot` 的执行地址是：

```text
AP_EXEC_MA = 0x8000
```

但是 `.ap_boot` 的内容实际随内核镜像一起加载。BSP 在启动 AP 前会调用：

```rust
copy_ap_boot_code()
```

把 `.ap_boot` 从内核镜像中的位置复制到：

```text
AP_BOOT_START_PA = 0x8000
```

然后 BSP 通过 INIT-SIPI-SIPI 唤醒 AP。AP 从低物理地址 `0x8000` 开始执行
AP boot 代码。

BSP 还会为 AP 填入：

```text
__ap_boot_info_array_pointer
__boot_page_table_pointer
```

其中 `__boot_page_table_pointer` 指向 boot page table 的物理地址。
AP 早期使用 boot page table 进入高半区，然后调用：

```rust
ap_early_entry(cpu_id)
```

AP 进入 Rust 后会：

```text
cpu::init_on_ap()
arch::enable_cpu_features()
trap::init_on_cpu()
activate_kernel_page_table()
arch::init_on_ap()
irq::enable_local()
boot_pt::dismiss()
等待 AP late entry
```

所以 AP 的地址路线是：

```text
低物理 0x8000
  -> AP boot temporary mapping
  -> 高半区 Rust ap_early_entry
  -> 正式 kernel page table
  -> kernel 注册的 ap_init()
```

## 14. 地址跳动时间线

下面用时间线总结 BSP 的关键地址变化。

```text
阶段 0：QEMU/OVMF/GRUB 准备启动

  QEMU 提供虚拟硬件。
  OVMF 提供 UEFI firmware。
  GRUB 从 rescue ISO 中启动。

  本命令中，GRUB 配置使用 linux/initrd：
    linux /boot/aster-kernel-osdk-bin ...
    initrd /boot/initramfs.cpio.gz

  /boot/aster-kernel-osdk-bin 是 OSDK 生成的 EFI64 bzImage，
  不是裸 ELF。


阶段 1：进入 bzImage EFI handover64 setup

  GRUB/UEFI 调用 entry_efi_handover64。
  setup 代码处理：
    boot_params
    cmdline
    initrd
    ACPI RSDP
    framebuffer
    EFI memory map -> E820 table

  然后退出 EFI boot services。


阶段 2：setup 跳入 Asterinas ELF

  目标地址:
    0x0800_1200 = __linux64_boot

  此时：
    rsi = boot_params ptr


阶段 3：Asterinas ELF 已按链接布局加载

  普通内核段 LMA: 0x0800_0000 起
  普通内核段 VMA: 0xffff_ffff_8800_0000 起
  BSP boot 段:     0x0800_1000 起


阶段 4：进入 .bsp_boot 的 __linux64_boot

  RIP: 低物理地址附近
  RSP: boot_stack_top 低地址
  页表: EFI/setup 阶段页表，随后被 Asterinas 临时页表替换


阶段 5：建立临时 boot page table

  建立：
    低 4 GiB identity mapping
    0xffff_8000_0000_0000 起的早期 linear mapping
    0xffff_ffff_8000_0000 起的 kernel code mapping


阶段 6：切换到 Asterinas 临时页表

  CR3: boot_l4pt
  低地址代码仍可通过 identity mapping 继续执行


阶段 7：long_mode_in_low_address

  RSP = RSP | 0xffff_ffff_8000_0000
  RIP = long_mode 的高半区地址


阶段 8：高半区 long_mode

  清 .bss
  设置 GS base
  根据 ENTRYTYPE_LINUX_64 调用 __linux_boot


阶段 9：Rust __linux_boot

  boot_params 中的物理地址
    -> paddr_to_vaddr()
    -> 0xffff_8000_0000_0000 + PA

  填充 EARLY_INFO
  调用 start_kernel()


阶段 10：OSTD init

  初始化早期 allocator、CPU、frame metadata、frame allocator
  创建正式 kernel page table


阶段 11：activate_kernel_page_table()

  CR3 切换到正式 kernel page table
  当前高半区 RIP/RSP 继续有效
  boot page table 后续 dismiss


阶段 12：进入 kernel crate

  __ostd_main()
    -> kernel::main()
    -> kernel::init::main()
```

## 15. 对 hvisor / 自定义 loader 的要求

如果不是通过 OSDK + GRUB/QEMU 的默认路径启动，而是由 hvisor 或自定义
loader 启动 Asterinas，需要特别注意以下点。

第一，先明确你要模拟哪一种启动对象。

如果你要模拟本文命令的行为，hvisor/loader 需要加载的是 OSDK 生成的
EFI64 bzImage，并按 Linux x86 Boot Protocol / EFI handover64 方式调用它。
这时 setup 代码会负责加载 Asterinas ELF payload，最终跳到 `0x0800_1200`。

如果你绕过 bzImage setup，直接加载 Asterinas ELF，那么必须按 ELF `PT_LOAD`
的 `p_paddr` / LMA 加载，或者保证等价布局。默认 x86_64 ELF 期望普通内核
物理加载在 `0x0800_0000` 附近，BSP boot 入口在 `0x0800_1000` 附近。

第二，BSP 入口必须匹配启动协议。

```text
本文命令等价路径:
  EFI handover64 bzImage -> setup -> 0x0800_1200 -> __linux64_boot

直接 ELF + Linux boot protocol:
  __linux32_boot 或 __linux64_boot

直接 ELF + Multiboot2:
  __multiboot_boot
```

第三，Linux boot protocol 路径必须准备有效的 `boot_params`。至少要确保
cmdline、initrd、E820 memory map、ACPI RSDP 等字段和 Asterinas 的
`linux_boot/mod.rs` 解析逻辑匹配。

第四，传给内核的 boot info、cmdline、initramfs 地址如果是物理地址，必须
在早期 low 4 GiB linear mapping 可覆盖的范围内，或者需要扩展早期页表和
解析逻辑。

第五，不能只把内核复制到任意物理地址然后跳到某个 Rust 函数。Asterinas
当前不是任意可重定位内核，`kernel_loaded_offset()` 也明确固定为
`KERNEL_CODE_BASE_VADDR`。

第六，如果要观察早期输出，x86_64 的 `early_println!` 写 COM1 串口。
QEMU/hvisor 需要给 guest 暴露对应 UART，并把输出连接到终端或日志文件。

## 16. 常用地址速查

| 地址 | 含义 |
| --- | --- |
| `0x0000_8000` | AP boot code 最终执行物理地址 |
| `0x0800_0000` | x86_64 kernel 物理加载基址 |
| `0x0800_1000` | BSP boot 段物理地址 |
| `0xffff_8000_0000_0000` | physical memory linear mapping 基址 |
| `0xffff_c000_0000_0000` | vmalloc / `KVirtArea` 基址 |
| `0xffff_e000_0000_0000` | frame metadata 基址 |
| `0xffff_ffff_8000_0000` | kernel code mapping 基址 / `KERNEL_VMA` |
| `0xffff_ffff_ffff_0000` | kernel address space 使用上界 |

## 17. 关键源码位置

| 文件 | 作用 |
| --- | --- |
| `osdk/src/base_crate/x86_64.ld.template` | x86_64 链接脚本，定义 LMA/VMA 和 section 布局 |
| `ostd/libs/linux-bzimage/setup/src/x86/header.S` | Linux x86 Boot Protocol header，声明 EFI handover64 支持 |
| `ostd/libs/linux-bzimage/setup/src/x86/amd64_efi/setup.S` | EFI handover64 / PE64 setup 入口 |
| `ostd/libs/linux-bzimage/setup/src/x86/amd64_efi/efi.rs` | EFI setup 主逻辑，整理 boot_params 并退出 boot services |
| `ostd/libs/linux-bzimage/setup/src/x86/amd64_efi/mod.rs` | setup 跳入 Asterinas `0x8001200` 的逻辑 |
| `ostd/src/arch/x86/boot/bsp_boot.S` | BSP 早期启动、临时页表、long mode、高半区跳转 |
| `ostd/src/arch/x86/boot/multiboot2/mod.rs` | Multiboot2 Rust 入口和 boot info 解析 |
| `ostd/src/arch/x86/boot/linux_boot/mod.rs` | Linux boot protocol Rust 入口和 boot params 解析 |
| `ostd/src/boot/mod.rs` | `EARLY_INFO` 和 `start_kernel()` |
| `ostd/src/lib.rs` | OSTD `crate::init()` 主流程 |
| `ostd/src/mm/kspace/mod.rs` | 正式 kernel address space 布局和正式页表建立 |
| `ostd/src/mm/page_table/boot_pt.rs` | boot page table 包装、扩展和释放 |
| `ostd/src/arch/x86/boot/smp.rs` | AP boot code 复制和 AP 唤醒 |
| `ostd/src/boot/smp.rs` | AP Rust 入口 `ap_early_entry()` |
| `kernel/src/lib.rs` | `#[ostd::main]` 入口 |
| `kernel/src/init.rs` | Asterinas kernel 初始化入口 |
