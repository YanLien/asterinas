# Asterinas 代码库总览

> Asterinas 是一个用 Rust 编写的内存安全、与 Linux 兼容的操作系统内核，基于创新的 **Framekernel** 架构。

本文档详细介绍四个顶层目录：`tools`、`kernel`、`osdk` 和 `ostd`。

---

## 目录

- [整体架构](#整体架构)
- [kernel/ — 内核主体](#kernel--内核主体)
- [ostd/ — 操作系统标准库](#ostd--操作系统标准库)
- [osdk/ — 操作系统开发套件](#osdk--操作系统开发套件)
- [tools/ — 开发工具集](#tools--开发工具集)

---

## 整体架构

```
┌─────────────────────────────────────────────┐
│                  kernel/                     │
│    进程 | 文件系统 | 网络 | 系统调用 | IPC   │
├─────────────────────────────────────────────┤
│                   ostd/                      │
│  内存管理 | 任务 | 同步 | 中断 | I/O | 架构  │
├─────────────────────────────────────────────┤
│    osdk/ (构建工具)    │  tools/ (开发基础设施)│
└─────────────────────────────────────────────┘
```

- **kernel/** 依赖 **ostd/** 进行所有硬件交互和安全抽象。
- **ostd/** 封装了全部 `unsafe` 代码，对外提供安全 API——这是 Asterinas 的小型可信计算基（TCB）。
- **osdk/** 是命令行构建/运行/测试工具链（类似于内核开发版的 Cargo）。
- **tools/** 提供 CI 脚本、代码质量工具、Docker 配置等基础设施。

支持的架构：**x86_64**、**RISC-V 64**、**LoongArch 64**。

---

## kernel/ — 内核主体

Asterinas 的核心内核，实现了与 Linux 兼容的操作系统功能。包含三个子目录：

### 目录结构

```
kernel/
├── src/          # 内核核心模块
├── comps/        # 系统组件与设备驱动
└── libs/         # 支撑库
```

### 核心模块 (`src/`)

#### `src/arch/` — 架构支持

每个架构（x86_64、riscv64、loongarch64）提供：

- CPU 管理与上下文切换
- 架构特定的信号处理
- 电源管理（关机/重启/停机）
- 系统调用入口/出口路径

#### `src/process/` — 进程管理

与 Linux 兼容的进程生命周期管理：

- PID 分配与管理
- 进程组、会话、作业控制
- 进程凭证（UID/GID/能力集）
- `clone` / `fork` / `execve` / `exit` 原语
- PID 命名空间和 IPC 命名空间

#### `src/thread/` — 线程管理

POSIX 线程实现：

- 线程创建、加入和终止
- CPU 亲和性（sched_setaffinity / sched_getaffinity）
- FPU/SIMD 上下文切换
- 线程本地存储（TLS）

#### `src/vm/` — 虚拟内存

- **VMAR**（虚拟内存地址区域）：表示进程的完整地址空间
- **VMO**（虚拟内存对象）：表示逻辑上连续的物理帧
- 内存映射、保护和解除映射
- 缺页中断处理
- 全局帧分配器和堆分配器接口

#### `src/fs/` — 文件系统

具有 Unix 语义的 VFS（虚拟文件系统）层：

- 文件操作：open、read、write、close、stat、ioctl、mmap 等
- 目录操作
- 挂载/卸载管理
- 支持的文件系统：**ext2**、**procfs**、**sysfs**、**tmpfs**、**ramfs**、**devpts**
- 管道（Pipe）和 FIFO 实现
- 每进程文件描述符表
- RootFS 初始化

#### `src/sched/` — 调度器

- **SCHED_FIFO**：实时先进先出调度策略
- **SCHED_RR**：实时时间片轮转调度策略
- **SCHED_OTHER**：默认分时调度策略，支持 nice 值
- 每 CPU 运行队列，支持 SMP 可扩展性
- 系统负载均值计算

#### `src/syscall/` — 系统调用

已实现 **230+** 个 Linux 系统调用：

- 架构特定的系统调用分发表
- 遵循 Linux ABI 约定的参数传递
- 覆盖范围：文件 I/O、进程管理、内存、信号、网络、IPC 等

#### `src/time/` — 时间管理

- `CLOCK_REALTIME` 和 `CLOCK_MONOTONIC` 实现
- 高精度定时器（hrtimer）
- 每进程和每线程的 CPU 时间统计
- `clock_gettime`、`nanosleep`、`gettimeofday` 等

#### `src/net/` — 网络栈

- 网络设备抽象层
- 套接字类型：Unix 域套接字、VSOCK、netlink
- UTS 命名空间（主机名/域名）
- 网络子系统初始化与配置

#### `src/ipc/` — 进程间通信

- SYSV 信号量，支持命名空间隔离
- IPC 命名空间支持
- 基于键值的 IPC 机制

#### `src/signal/` — 信号处理

- 完整的 POSIX 信号投递机制
- 信号掩码和待处理信号队列
- 实时信号支持

#### 其他核心模块

| 模块 | 说明 |
|------|------|
| `src/device/` | 设备注册与 `/dev` 文件系统集成 |
| `src/driver/` | TTY 和 PTY 终端设备驱动 |
| `src/context.rs` | 安全的用户空间内存访问原语 |
| `src/error.rs` | 与 Linux 兼容的 `errno` 错误码 |
| `src/security/` | Intel TDX 机密计算支持 |
| `src/vdso.rs` | 虚拟动态共享对象，用于优化系统调用开销 |

### 系统组件 (`comps/`)

| 组件 | 说明 |
|------|------|
| `aster-pci` | PCI 总线枚举与设备管理 |
| `aster-virtio` | VirtIO 虚拟设备（网络、块设备、控制台、输入） |
| `aster-block` | 块设备框架 |
| `aster-uart` | UART 串口通信 |
| `aster-console` | 控制台与终端管理 |
| `aster-framebuffer` | 图形帧缓冲支持 |
| `aster-input` | 输入设备处理 |
| `aster-i8042` | PS/2 键盘控制器驱动 |
| `aster-mlsdisk` | 多层安全磁盘存储（用于可信执行环境 TEE） |
| `aster-softirq` | 软中断（中断下半部）处理 |
| `aster-systree` | 系统设备树管理 |
| `aster-time` | 时钟与定时器管理 |
| `aster-cmdline` | 内核命令行参数解析 |
| `aster-logger` | 内核日志框架 |

### 支撑库 (`libs/`)

| 库 | 说明 |
|----|------|
| `aster-bigtcp` | TCP/IP 网络协议栈（基于 smoltcp） |
| `aster-rights` | 基于能力的访问控制 |
| `aster-rights-proc` | 进程级权限管理（过程宏） |
| `aster-util` | 通用工具函数与辅助设施 |
| `xarray` | 扩展数组数据结构（类似 Linux 的 xarray） |
| `lru` | LRU 缓存实现 |
| `jhash` | Jenkins 哈希算法 |
| `keyable-arc` | 带键访问的引用计数智能指针 |
| `atomic-integer-wrapper` | 原子整数工具 |
| `typeflags` | 类型级标志位宏 |
| `bitvec` | 位向量操作 |
| `zerocopy` | 零拷贝内存操作 |
| `id-alloc` | ID 分配管理 |
| `align_ext` | 对齐工具 |
| `device-id` | 设备标识 |
| `inherit-methods-macro` | 方法继承过程宏 |
| `paste` | 宏中的 token 粘贴工具 |
| `controlled` | 受控执行框架 |
| `logo-ascii-art` | ASCII 艺术标志渲染 |

---

## ostd/ — 操作系统标准库

底层 OS 框架，封装了所有硬件交互。**全部 `unsafe` 代码被限制在此框架内部**，向内核层提供安全的 Rust API。这是 Asterinas 的小型可信计算基（TCB）。

### 目录结构

```
ostd/
├── src/          # 核心框架模块
│   ├── arch/     # 架构特定实现
│   ├── mm/       # 内存管理
│   ├── task/     # 任务/线程管理
│   ├── sync/     # 同步原语
│   ├── io/       # 设备 I/O
│   ├── irq/      # 中断处理
│   ├── cpu/      # CPU 管理
│   └── ...
└── libs/         # 支撑库
```

### 核心模块 (`src/`)

#### `src/arch/` — 架构实现

| 架构 | 功能特性 |
|------|----------|
| **x86_64** | CPU 上下文、分页、APIC、IOMMU、ACPI、TDX（机密虚拟机）、SMP 启动、I/O 端口 |
| **RISC-V** | CPU 上下文、分页、PLIC 中断控制器、SBI 接口、SMP 启动 |
| **LoongArch** | CPU 上下文、分页、设备中断、SMP 启动 |

#### `src/mm/` — 内存管理

| 子模块 | 说明 |
|--------|------|
| `frame/` | 物理帧分配与释放 |
| `heap/` | 内核堆，基于 slab 分配器 |
| `page_table/` | 硬件页表管理 |
| `vm_space/` | 虚拟地址空间管理 |
| `dma/` | DMA 一致性内存处理 |
| `kspace/` | 内核地址空间布局 |
| `io/` | 内存映射 I/O 操作 |

#### `src/task/` — 任务/线程管理

| 子模块 | 说明 |
|--------|------|
| `scheduler/` | FIFO 调度器实现 |
| `processor/` | CPU 处理器上下文 |
| `preempt/` | 抢占保护与处理 |
| `atomic_mode/` | 原子（不可抢占）执行模式 |
| `kernel_stack/` | 内核栈分配 |

#### `src/sync/` — 同步原语

| 原语 | 说明 |
|------|------|
| `mutex/` | 互斥锁 |
| `rwlock/` | 读写锁 |
| `spin/` | 自旋锁 |
| `rcu/` | Read-Copy-Update 机制 |
| `wait/` | 等待队列 |

#### `src/io/` — 设备 I/O

| 子模块 | 说明 |
|--------|------|
| `io_mem/` | 内存映射 I/O 地址分配器 |
| `io_port/` | 端口 I/O 分配器（仅 x86_64） |

#### `src/irq/` — 中断处理

| 子模块 | 说明 |
|--------|------|
| `chip/` | 中断控制器抽象（架构特定） |
| `top_half.rs` | 上半部（硬中断）处理 |
| `bottom_half.rs` | 下半部（软中断）处理 |
| `ipi/` | 核间中断（IPI） |

#### `src/cpu/` — CPU 管理

| 子模块 | 说明 |
|--------|------|
| `id.rs` | CPU ID 查询 |
| `local/` | CPU 本地存储 |
| `cell.rs` | CPU 本地 Cell（安全的每 CPU 数据） |

#### 其他模块

| 模块 | 说明 |
|------|------|
| `log/` | 内核日志系统，8 个级别（emerg、alert、crit、error、warn、notice、info、debug） |
| `boot/` | 启动内存区域与 SMP 协调 |
| `timer/` | 基于 jiffies 的定时器 |
| `user/` | 用户态-内核态空间转换 |
| `console/` | UART 控制台输出（NS16550A） |
| `bus.rs` | 系统总线抽象 |
| `power.rs` | 系统电源控制（停机/重启） |
| `panic.rs` | 自定义 panic 处理器 |
| `error.rs` | OSTD 通用错误类型 |
| `ex_table.rs` | 异常表（用于故障恢复） |
| `util/` | 范围分配器、ID 集合、Either 类型、工具宏 |

### 支撑库 (`libs/`)

| 库 | 说明 |
|----|------|
| `ostd-pod` | `Pod` trait，用于安全的字节级转换；支持 `#[derive(Pod)]`、`#[pod_union]`、`#[padding_struct]` |
| `ostd-macros` | 过程宏：`#[ostd::main]`、`#[global_frame_allocator]`、`#[global_heap_allocator]`、`#[ktest]`、`#[panic_handler]` |
| `ostd-test` | 内核测试框架——为 `#![no_std]` crate 提供 `cargo test` 般的体验 |
| `int-to-c-enum` | `TryFromInt` 派生宏，用于安全的整数到枚举转换 |
| `padding-struct` | 为 `#[repr(C)]` 结构体添加显式填充 |
| `id-alloc` | 基于 bitmap 的 ID 分配器（支持连续 ID 范围） |
| `align-ext` | `align_up` / `align_down` 幂次对齐工具 |
| `linux-bzimage` | Linux bzImage 启动格式支持（UEFI + 传统启动，PE/COFF 头生成） |

---

## osdk/ — 操作系统开发套件

命令行工具链（类似于内核开发版的 Cargo），管理 Asterinas 内核的构建、运行、测试和调试。

### 目录结构

```
osdk/
├── src/          # CLI 与命令实现
├── deps/         # 内置依赖 crate
├── tests/        # 测试套件与教程示例
└── tools/        # 开发环境 Docker 配置
```

### 命令一览

| 命令 | 说明 |
|------|------|
| `osdk new` | 创建新的内核/库/模块项目 |
| `osdk build` | 编译内核项目 |
| `osdk run` | 在 QEMU 中运行内核 |
| `osdk debug` | 通过 GDB 调试运行中的内核 |
| `osdk test` | 在虚拟机中运行内核单元测试 |
| `osdk check` | 执行 `cargo check` 代码分析 |
| `osdk clippy` | 运行 clippy 代码检查 |
| `osdk doc` | 构建 Rust 文档 |
| `osdk profile` | 性能分析 |

### 源码结构 (`src/`)

| 模块 | 说明 |
|------|------|
| `main.rs` | 入口，初始化日志后调用 CLI |
| `cli.rs` | 命令行参数解析（基于 clap） |
| `commands/build/` | 构建命令实现 |
| `commands/run/` | 运行命令（调用 QEMU） |
| `commands/debug/` | 调试命令（GDB 集成） |
| `commands/test/` | 测试命令（内核测试执行） |
| `commands/new/` | 项目脚手架 |
| `commands/profile/` | 性能分析 |
| `config/manifest.rs` | `OSDK.toml` 清单解析 |
| `config/scheme/` | 配置方案：启动、构建、运行、测试 |
| `bundle/bin/` | 二进制输出格式（ELF、bzImage） |
| `bundle/vm_image/` | 虚拟机镜像创建（QCOW2、GRUB） |
| `base_crate/` | 内核入口的基 crate 管理 |
| `arch.rs` | 架构支持（x86_64、aarch64、riscv64、loongarch64） |

### 内置依赖 (`deps/`)

| Crate | 说明 |
|-------|------|
| `frame-allocator` | Buddy system 物理帧分配器；无堆实现，使用 OSTD 帧元数据，带每 CPU 缓存 |
| `heap-allocator` | 基于 slab 的全局堆分配器；每 CPU 缓存，依赖 OSTD slab 机制 |
| `test-kernel` | 基于 OSTD 的最小化内核，用于运行单元测试；提供默认测试基础设施 |

### 配置文件 (`OSDK.toml`)

```toml
[project]
name = "my-kernel"
version = "0.1.0"
type = "kernel"           # kernel（内核） | library（库） | module（模块）

[boot]
method = "grub"           # grub（GRUB 引导） | direct（直接引导）
protocol = "linux"        # linux | multiboot

[build]
opt-level = 2

[run]
qemu-args = ["-m", "512M"]

[test]
qemu-args = ["-m", "256M"]
```

---

## tools/ — 开发工具集

辅助开发、CI/CD 和测试的各类脚本与工具。

### sctrace — 系统调用兼容性追踪器

专用工具，用于检测 Linux 应用与 Asterinas 的兼容性：

- **在线模式**：通过 `strace` 实时追踪运行中的程序
- **离线模式**：分析已有的 `strace` 日志文件
- 将系统调用与 **SCML**（System Call Matching Language，系统调用匹配语言）模式文件进行匹配
- 核心组件：
  - `scml_parser.rs` — 解析 SCML 模式文件
  - `scml_matcher.rs` — 将系统调用与模式进行匹配
  - `strace_parser.rs` — 解析 `strace` 输出格式

用法：
```bash
# 在线模式
sctrace <scml文件>... -- <程序> [参数]
# 离线模式
sctrace <scml文件>... --input <strace日志>
```

### CI/CD 脚本

| 脚本 | 说明 |
|------|------|
| `publish_sctrace.sh` | 发布 sctrace 到 crates.io |
| `publish_osdk_and_ostd.sh` | 发布 OSDK 和 OSTD 包 |
| `publish_api_docs.sh` | 发布 API 文档 |
| `prepare_for_docker_build_and_push.sh` | 准备并推送 Docker 镜像 |

### 代码质量工具

| 脚本 | 说明 |
|------|------|
| `format_all.sh` | 格式化工作区中所有 Rust 代码（加 `--check` 用于 CI 验证） |
| `clippy_check.sh` | 以 `osdk` 或 `workspace` 模式运行 clippy |
| `bump_version.sh` | 在整个代码库中升级版本号（major/minor/patch/date） |
| `print_workspace_members.sh` | 列出 Cargo workspace 成员 |

### 网络与 QEMU 配置

| 脚本 | 说明 |
|------|------|
| `net/qemu-ifup.sh` | 创建 QEMU 网络的 TAP 接口（10.0.2.2/24） |
| `net/qemu-ifdown.sh` | 清理 TAP 接口 |
| `qemu_args.sh` | 生成 QEMU 启动参数（方案：normal、test、microvm、iommu） |

### NixOS 集成

| 脚本 | 说明 |
|------|------|
| `nixos/build_nixos.sh` | 构建基于 NixOS 的 Asterinas 磁盘镜像 |
| `nixos/build_iso.sh` | 构建 NixOS ISO 镜像 |
| `nixos/run.sh` | 运行构建好的 NixOS 镜像 |

### Docker 环境

- 提供 Asterinas 开发环境的 Dockerfile
- 基于 `asterinas/osdk` 基础镜像，集成 Nix 包管理器
- 多平台支持（linux/amd64、linux/arm64）

### 其他工具

| 脚本 | 说明 |
|------|------|
| `atomic_wget.sh` | 原子性文件下载（临时文件 + 重命名模式，确保文件完整或不出现） |
| `sctrace.sh` | 从仓库中运行 sctrace 的包装脚本 |

---

## 总结

| 目录 | 角色 | 核心特征 |
|------|------|----------|
| `kernel/` | 操作系统内核实现 | 与 Linux 兼容、内存安全、230+ 系统调用 |
| `ostd/` | 底层 OS 框架 | 封装所有 `unsafe` 代码、提供安全 API、小型 TCB |
| `osdk/` | 构建与开发工具链 | 类 Cargo 的内核开发 CLI |
| `tools/` | 开发基础设施 | CI、测试、Docker、代码质量 |

设计哲学：**kernel** 专注操作系统逻辑，**ostd** 提供安全的硬件抽象，**osdk** 简化开发工作流，**tools** 处理周边基础设施。
