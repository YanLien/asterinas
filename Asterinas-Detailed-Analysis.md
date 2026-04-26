# Asterinas 项目详细解析

> Asterinas 是一个基于 Rust 的 Linux 兼容内核，采用 Framekernel 架构。
> `unsafe` Rust 被限制在 OSTD (`ostd/`) 中，内核 (`kernel/`) 完全是安全 Rust。

---

## 目录

- [1. 项目结构总览](#1-项目结构总览)
- [2. OSDK 框架详解](#2-osdk-框架详解)
  - [2.1 架构概述](#21-架构概述)
  - [2.2 支持的命令](#22-支持的命令)
  - [2.3 配置继承体系](#23-配置继承体系)
  - [2.4 OSDK.toml 参数详解](#24-osdktoml-参数详解)
  - [2.5 构建、运行、测试的工作流程](#25-构建运行测试的工作流程)
- [3. Makefile 命令](#3-makefile-命令)
- [4. ostd — OS 框架底层](#4-ostd--os-框架底层)
  - [4.1 启动流程](#41-启动流程)
  - [4.2 核心模块](#42-核心模块)
  - [4.3 架构特定代码](#43-架构特定代码)
  - [4.4 附带库](#44-附带库)
- [5. kernel — 安全 Rust 内核](#5-kernel--安全-rust-内核)
  - [5.1 入口](#51-入口)
  - [5.2 process — 进程管理](#52-process--进程管理)
  - [5.3 fs — 文件系统](#53-fs--文件系统)
  - [5.4 vm — 虚拟内存](#54-vm--虚拟内存)
  - [5.5 syscall — 系统调用](#55-syscall--系统调用)
  - [5.6 net — 网络栈](#56-net--网络栈)
  - [5.7 device — 设备管理](#57-device--设备管理)
  - [5.8 sched — 调度器](#58-sched--调度器)
  - [5.9 ipc — 进程间通信](#59-ipc--进程间通信)
  - [5.10 comps — 内核组件](#510-comps--内核组件)
  - [5.11 libs — 内核库](#511-libs--内核库)
- [6. osdk — 开发工具与内核空间库](#6-osdk--开发工具与内核空间库)
  - [6.1 cargo-osdk CLI 工具](#61-cargo-osdk-cli-工具)
  - [6.2 frame-allocator — 物理页帧分配器](#62-frame-allocator--物理页帧分配器)
  - [6.3 heap-allocator — 内核堆分配器](#63-heap-allocator--内核堆分配器)
  - [6.4 test-kernel — 内核态测试运行器](#64-test-kernel--内核态测试运行器)
- [7. 三者关系总结](#7-三者关系总结)

---

## 1. 项目结构总览

### 根目录文件与目录

| 目录/文件 | 作用 |
|-----------|------|
| `kernel/` | 安全 Rust 内核（syscalls, VFS, 网络等），`#![deny(unsafe_code)]` |
| `ostd/` | OS 框架底层（内存管理、页表、中断、调度等），**唯一允许 `unsafe` 的 crate** |
| `osdk/` | `cargo-osdk` CLI 工具源码 + 内核空间依赖库（frame-allocator, heap-allocator, test-kernel） |
| `test/` | 测试：C 用户态程序、initramfs、回归测试、一致性测试、NixOS 测试 |
| `tools/` | 实用脚本（格式化、Docker、QEMU 参数、基准测试等） |
| `book/` | Asterinas Book 文档（mdBook） |
| `distro/` | Asterinas NixOS 发行版配置（Nix 文件） |
| `.github/` | GitHub CI/CD workflows 和 issue/PR 模板 |
| `Cargo.toml` | Workspace 根配置，定义所有成员 crate |
| `Cargo.lock` | 锁定依赖版本 |
| `OSDK.toml` | OSDK manifest，定义构建/运行/测试的默认配置和各 scheme |
| `Makefile` | 顶层构建入口 |
| `rust-toolchain.toml` | 固定 Rust nightly 版本（nightly-2025-12-06） |
| `rustfmt.toml` | Rust 格式化规则 |
| `VERSION` | 项目版本号（`0.17.1`） |
| `Components.toml` | kernel 组件的名称映射和白名单 |
| `DOCKER_IMAGE_VERSION` | Docker 镜像版本号 |
| `.typos.toml` | typos 拼写检查的忽略规则 |
| `.licenserc.yaml` | License Eye Header 检查配置 |
| `triagebot.toml` | Rust triagebot 配置 |
| `CODEOWNERS` | GitHub 代码审阅负责人分配 |

### 架构层次

```
开发者 → cargo osdk build/run/test
              │
         ┌────▼──────┐
         │ cargo-osdk│  ← 主机端 CLI 工具 (osdk/)
         └────┬──────┘
              │  读取 OSDK.toml, 生成 base crate,
              │  调用 cargo/qemu/grub
    ┌─────────┼─────────┐
    ▼         ▼         ▼
  ostd/    osdk/deps/*  kernel/
 (unsafe   (frame-alloc, (safe Rust
  only)     heap-alloc,   内核)
            test-kernel)
```

---

## 2. OSDK 框架详解

### 2.1 架构概述

**OSDK** (Operating System Development Kit) 是一个 Cargo 子命令工具 (`cargo-osdk`)，用于简化基于 **framekernel 架构**的 Rust OS 开发。它处理内核的构建、运行、测试、调试和性能分析。

**核心要点**：
- `osdk/src/` 不在 workspace 成员中（是主机端工具，不是内核代码）
- `osdk/deps/` 是内核空间库（frame allocator, heap allocator, test framework），作为 workspace 成员
- OSDK 通过生成临时的 "base crate" 来桥接 Cargo 的常规构建系统和裸机内核目标

### 2.2 支持的命令

| 命令 | 功能 |
|------|------|
| `cargo osdk new` | 创建新的内核或库项目 |
| `cargo osdk build` | 编译内核 |
| `cargo osdk run` | 在 QEMU 中运行内核 |
| `cargo osdk test` | 在 QEMU 中运行内核态单元测试 |
| `cargo osdk debug` | 通过 GDB 远程调试 |
| `cargo osdk profile` | 火焰图性能分析 |
| `cargo osdk check` | 检查错误（转发到 cargo check） |
| `cargo osdk clippy` | Lint 检查（转发到 cargo clippy） |
| `cargo osdk doc` | 构建文档 |

### 2.3 配置继承体系

配置解析的核心数据流：

```
OSDK.toml 文件 → TomlManifest → Scheme → Config
                    │              │
              default_scheme   scheme map (named schemes)
                    │              │
                    └──继承────────┘

继承规则:
  1. run/test action 继承全局 boot/grub/qemu/build 设置
  2. named scheme 继承 default scheme 的所有设置
  3. CLI 参数覆盖 manifest 中的值
  4. QEMU args 支持通过 bash shell 求值（支持环境变量和命令替换）
```

**Scheme 结构体（核心配置对象）**：

```rust
pub struct Scheme {
    pub work_dir: Option<PathBuf>,
    pub supported_archs: Vec<Arch>,
    pub boot: Option<BootScheme>,       // 启动配置
    pub grub: Option<GrubScheme>,       // GRUB 配置
    pub qemu: Option<QemuScheme>,       // QEMU 配置
    pub build: Option<BuildScheme>,     // 编译配置
    pub run: Option<ActionScheme>,      // 运行专属覆盖
    pub test: Option<ActionScheme>,     // 测试专属覆盖
}
```

### 2.4 OSDK.toml 参数详解

#### 顶层字段

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `project_type` | `"kernel"` / `"library"` | 自动推断（有 `#[ostd::main]` → kernel） | 项目类型 |
| `supported_archs` | `["x86_64","riscv64",...]` | 所有架构 | 限制支持的架构列表 |

#### `[boot]` 启动配置

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `method` | `"qemu-direct"` / `"grub-rescue-iso"` / `"grub-qcow2"` | `"qemu-direct"` | 启动方式 |
| `kcmd_args` | `["KEY=VALUE", ...]` | `[]` | 传递给 guest 内核的命令行参数，可重复（追加继承） |
| `init_args` | `["arg1", ...]` | `[]` | 传递给 init 进程的参数，可重复（追加继承） |
| `initramfs` | `"path/to/file"` | 无 | initramfs 路径（相对路径基于 OSDK.toml 所在目录，覆盖继承） |

**`kcmd_args` 与 `init_args` 的区别**：

`kcmd_args` 传给**内核**，相当于 Linux 的内核启动参数（kernel cmdline），格式是 `KEY=VALUE`。内核启动时解析这些键值对来配置自身行为。

`init_args` 传给 **init 进程**（用户态第一个进程），相当于内核启动完成后执行 `init` 程序的命令行参数。

源码中的处理：

```rust
pub fn finalize(self) -> Boot {
    let mut kcmdline = self.kcmd_args;
    kcmdline.push("--".to_owned());     // "--" 分隔内核参数和 init 参数
    kcmdline.extend(self.init_args);
    // ...
}
```

最终生成的完整 cmdline 类似：

```
SHELL=/bin/sh HOME=/ console=hvc0 ostd.log_level=error -- sh -l
         ↑ 内核参数部分 ↑              ↑ 分隔符 ↑  ↑ init 参数 ↑
```

#### `[grub]` GRUB 配置（仅 grub-rescue-iso / grub-qcow2 时生效）

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `boot_protocol` | `"multiboot2"` / `"multiboot"` / `"linux"` | `"multiboot2"` | GRUB 引导协议（覆盖继承） |
| `mkrescue_path` | `"path"` | 系统 PATH 中的 `grub-mkrescue` | grub-mkrescue 可执行文件路径（覆盖继承） |
| `display_grub_menu` | `bool` | `false` | 是否显示 GRUB 菜单（**不继承**） |

#### `[qemu]` QEMU 配置

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `args` | `"string"` | `""` | QEMU 额外参数，**支持 shell 求值**（环境变量、`$()`命令替换），覆盖继承 |
| `path` | `"path"` | 架构对应的系统 `qemu-system-*` | QEMU 可执行文件路径，覆盖继承 |
| `bootdev_append_options` | `",if=virtio,..."` | 无 | `-drive` 启动设备的额外选项（仅 grub 方式生效） |
| `with_monitor` | `bool` | `false` | 以 daemon 模式运行 QEMU，连接 monitor |

`args` 的限制：
- **禁止设置**：`-kernel`, `-append`, `-initrd`（OSDK 自动管理）
- **单值 key**：`-cpu`, `-machine`, `-m`, `-serial`, `-monitor`, `-display`
- **多值 key**：`-device`, `-chardev`, `-object`, `-netdev`, `-drive`, `-cdrom`
- **无值 key**：`--no-reboot`, `-nographic`, `-enable-kvm`

#### `[build]` 编译配置

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `profile` | `"dev"` / `"release"` / `"test"` / `"bench"` | `"dev"` | Cargo build profile（覆盖继承） |
| `features` | `["feat1", ...]` | `[]` | 激活的 Cargo features，追加继承 |
| `no_default_features` | `bool` | `false` | 禁用默认 features（**不继承**） |
| `strip_elf` | `bool` | `false` | 用 `rust-strip` 裁减内核 ELF |
| `encoding` | `"raw"` / `"gzip"` / `"zlib"` | `"raw"` | 内核自解压编码格式（仅 Linux boot protocol 时可用） |
| `linux_x86_legacy_boot` | `bool` | `false` | 启用 Linux x86 32 位传统启动协议 |
| `rustflags` | `"string"` | `""` | 额外 RUSTFLAGS，追加继承 |

#### `[run]` 运行专属覆盖

`[run]` 下可设 `[run.boot]`、`[run.grub]`、`[run.qemu]`、`[run.build]`，字段与上面完全相同。**仅 `cargo osdk run` 时生效**。未指定的字段从全局 `[boot]`/`[grub]`/`[qemu]`/`[build]` 继承。

#### `[test]` 测试专属覆盖

同 `[run]`，但**仅 `cargo osdk test` 时生效**。

#### `[scheme."name"]` 自定义方案

拥有和默认 scheme 完全相同的结构，通过 `--scheme name` 选择。继承规则：
1. `scheme.name.run/test` 继承 `scheme.name` 全局
2. `scheme.name` 整体继承 default scheme
3. `supported_archs` 和 `display_grub_menu` **不继承**

#### 继承规则汇总

| 字段 | 继承方式 |
|------|---------|
| `boot.method` | 覆盖 |
| `boot.initramfs` | 覆盖 |
| `boot.kcmd_args` | 追加（父在前，子在后） |
| `boot.init_args` | 追加 |
| `grub.boot_protocol` | 覆盖 |
| `grub.mkrescue_path` | 覆盖 |
| `grub.display_grub_menu` | **不继承** |
| `qemu.args` | 覆盖 |
| `qemu.path` | 覆盖 |
| `qemu.with_monitor` | 覆盖 |
| `qemu.bootdev_append_options` | 覆盖 |
| `build.profile` | 覆盖 |
| `build.features` | 追加 |
| `build.no_default_features` | **不继承** |
| `build.strip_elf` | 父子任一为 true 则 true |
| `build.linux_x86_legacy_boot` | 父子任一为 true 则 true |
| `build.encoding` | 覆盖 |
| `build.rustflags` | 追加（父 + 空格 + 子） |
| `supported_archs` | **不继承** |

#### 优先级（从高到低）

```
CLI 参数 (--qemu-args, --boot-method, ...)
    ↓ 覆盖
[scheme."xxx".run/test]  (当前 scheme 的 action 级别)
    ↓ 继承
[scheme."xxx"]           (当前 scheme 的全局设置)
    ↓ 继承
[run] / [test]           (默认 scheme 的 action 级别)
    ↓ 继承
[boot] / [grub] / [qemu] / [build]  (默认 scheme 全局)
    ↓ 默认值
字段硬编码默认值
```

### 2.5 构建、运行、测试的工作流程

#### `make run_kernel` 完整构建流程

```
make run_kernel
  │
  ├── 1. 检查安装 cargo-osdk
  │     └── 如果 ~/.cargo/bin/cargo-osdk 不存在或 osdk/ 源码有更新
  │         → cargo install --path osdk
  │
  ├── 2. 构建 initramfs
  │     ├── 检查 VDSO_LIBRARY_DIR 环境变量
  │     └── make -C test/initramfs
  │         → 编译 C 测试程序 → 打包为 initramfs.cpio.gz
  │
  └── 3. cd kernel && cargo osdk run $(CARGO_OSDK_BUILD_ARGS)
        │
        │  ┌─── OSDK 内部流程 ───────────────────────────────┐
        │  │                                                    │
        │  │  A. 加载配置                                        │
        │  │     TomlManifest::load()                            │
        │  │     ├── 读取 OSDK.toml                             │
        │  │     ├── 解析 default_scheme + named schemes         │
        │  │     ├── run/test 继承全局设置                        │
        │  │     └── named scheme 继承 default scheme            │
        │  │     → 合并 CLI 参数 → 生成最终 Config               │
        │  │                                                    │
        │  │  B. 生成 base crate (new_base_crate)               │
        │  │     目标: target/osdk/aster-kernel-run-base/        │
        │  │     ├── Cargo.toml (依赖 aster-kernel + ostd 等)    │
        │  │     ├── src/main.rs (调用内核入口)                   │
        │  │     ├── x86_64.ld / riscv64.ld / loongarch64.ld    │
        │  │     ├── 复制 workspace 的 profile 配置               │
        │  │     └── 复制 target crate 的 features               │
        │  │                                                    │
        │  │  C. 缓存检查                                        │
        │  │     ├── 检查已有 bundle 是否可复用                    │
        │  │     ├── 比较配置 (boot/grub/qemu/build) 是否一致     │
        │  │     └── 比较源码修改时间 vs bundle 修改时间           │
        │  │     → 可复用则跳过构建，直接运行                      │
        │  │                                                    │
        │  │  D. 编译内核 ELF (build_kernel_elf)                │
        │  │     cargo build                                     │
        │  │     --target x86_64-unknown-none                    │
        │  │     RUSTFLAGS:                                      │
        │  │       -C link-arg=-Tx86_64.ld                       │
        │  │       -C relocation-model=static                    │
        │  │       -C no-redzone=y                               │
        │  │       -C force-unwind-tables=yes                    │
        │  │       --check-cfg cfg(ktest)                        │
        │  │     → 产出: target/x86_64-unknown-none/dev/aster-kernel │
        │  │                                                    │
        │  │  E. 打包 (根据 boot.method 分支)                    │
        │  │                                                    │
        │  │     grub-rescue-iso (默认):                         │
        │  │       1. 创建 iso_root/ 目录结构                    │
        │  │       2. 生成 grub.cfg (根据 boot_protocol)         │
        │  │       3. grub-mkrescue → aster-kernel.iso           │
        │  │       4. 保存到 bundle/                             │
        │  │                                                    │
        │  │     qemu-direct:                                    │
        │  │       Linux 协议: ELF → 编码压缩 → bzImage          │
        │  │       其他协议: ELF → 复制/strip → 修改 em_machine  │
        │  │                                                    │
        │  │     grub-qcow2:                                     │
        │  │       同 grub-rescue-iso → convert ISO→QCOW2       │
        │  │                                                    │
        │  │  F. 运行 QEMU (Bundle::run)                        │
        │  │     qemu-system-x86_64                             │
        │  │       -drive file=aster-kernel.iso,format=raw,...  │
        │  │       -accel kvm                                   │
        │  │       -m 8G -smp 1 ...                             │
        │  │                                                    │
        │  └────────────────────────────────────────────────────┘
```

---

## 3. Makefile 命令

### 核心开发命令

| 命令 | 说明 |
|------|------|
| `make all` / `make kernel` | 构建 initramfs + 内核 |
| `make run_kernel` | 构建并运行内核（QEMU） |
| `make initramfs` | 单独构建 initramfs |

### 测试命令

| 命令 | 说明 |
|------|------|
| `make test` | 非 OSDK crate 的单元测试（`cargo test`） |
| `make ktest` | 内核态单元测试（通过 `cargo osdk test` 在 QEMU 中运行） |

### 检查与格式化

| 命令 | 说明 |
|------|------|
| `make check` | 完整 lint：rustfmt + clippy + typos + license + Nix 检查 |
| `make format` | 自动格式化 Rust、Nix、C 代码 |
| `make docs` | 构建所有 crate 的 rustdoc |
| `make book` | 构建 Asterinas Book（mdBook） |

### 调试与性能分析

| 命令 | 说明 |
|------|------|
| `make gdb_server` | 启动 QEMU 并开启 GDB server（等待客户端连接） |
| `make gdb_client` | 启动 GDB 客户端连接到远程内核 |
| `make profile_server` | 启动 QEMU 用于性能分析 |
| `make profile_client` | 收集火焰图样本数据 |

### OSDK 工具本身

| 命令 | 说明 |
|------|------|
| `make install_osdk` | 从源码安装/更新 `cargo-osdk` |
| `make check_osdk` | 对 OSDK 自身运行 clippy |
| `make test_osdk` | 测试 OSDK 自身 |

### NixOS 相关

| 命令 | 说明 |
|------|------|
| `make iso` | 构建 NixOS ISO 安装镜像 |
| `make run_iso` | 运行 ISO 安装 |
| `make nixos` | 创建 NixOS 安装 |
| `make run_nixos` | 运行已安装的 NixOS |
| `make cachix` | 构建 NixOS 补丁包 |
| `make push_cachix` | 推送到 Cachix 缓存 |

### 其他

| 命令 | 说明 |
|------|------|
| `make clean` | 清理所有构建产物 |
| `make check_vdso` | 检查 VDSO_LIBRARY_DIR 环境变量 |

### 常用 Makefile 变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `OSDK_TARGET_ARCH` | `x86_64` | 目标架构 |
| `RELEASE` | `0` | Release 构建 |
| `RELEASE_LTO` | `0` | Release + LTO 构建 |
| `SMP` | `1` | CPU 核数 |
| `MEM` | `8G` | 内存大小 |
| `LOG_LEVEL` | `error` | 日志级别 |
| `CONSOLE` | `hvc0` | 控制台（tty0/ttyS0/hvc0） |
| `BOOT_METHOD` | `grub-rescue-iso` | 启动方式 |
| `BOOT_PROTOCOL` | `multiboot2` | 引导协议 |
| `SCHEME` | `""` | OSDK scheme |
| `ENABLE_KVM` | `1` | 启用 KVM 加速 |
| `INTEL_TDX` | `0` | Intel TDX 模式 |
| `AUTO_TEST` | `none` | 自动测试（conformance/regression/boot/vsock） |
| `COVERAGE` | `0` | 启用覆盖率 |
| `NETDEV` | `user` | 网络设备（user/tap） |
| `FEATURES` | | 额外 Cargo features |

---

## 4. ostd — OS 框架底层

**定位**：提供硬件抽象和核心 OS 原语，所有 unsafe 代码集中在此。是整个内核的基础。

### 4.1 启动流程

```
lib.rs: unsafe fn init()
  1. arch::enable_cpu_features()      ← 启用 CPU 特性
  2. 早期 frame allocator 初始化
  3. 串口控制台 + 日志系统
  4. per-CPU 本地存储
  5. frame 元数据 + 完整 frame allocator
  6. 内核页表 + 激活
  7. sync::init() (RCU)
  8. heap 初始化
  9. arch late init
  10. SMP 多核启动
  11. 释放 boot 页表
  12. 启用本地 IRQ
  13. 调用 .init_array (FFI 构造函数)
```

### 4.2 核心模块

#### mm — 内存管理

**类型定义**：

| 类型 | 说明 |
|------|------|
| `Vaddr` / `Paddr` / `Daddr` | 虚拟/物理/设备地址（`usize`） |
| `Frame` / `UFrame` | 物理页帧，带引用计数和元数据 |
| `Segment` / `USegment` | 连续物理页帧段 |
| `PageTable<C>` | 通用页表，通过 `PageTableConfig` trait 适配不同架构和用途 |
| `VmSpace` | 用户虚拟地址空间，包装用户页表 |
| `Cursor` / `CursorMut` | 页表游标，支持并发访问不同 VA 区间 |
| `VmReader` / `VmWriter` | 安全的内存读写器 |

**页表抽象**：

```
PageTableConfig (trait)  ← 区分内核页表和用户页表
  ├── KernelPtConfig  (管理高半部分地址)
  └── UserPtConfig    (管理低 256 项 = 用户空间)
PteTrait (trait)        ← 架构特定的 PTE 格式
  ├── x86_64: PageTableEntry (Present/R/W/US/PS bits...)
  ├── riscv64: PageTableEntry (SV39/SV48)
  └── loongarch: PageTableEntry
```

**关键类型详解**：

`PageTable<C>` — 通用页表：

| 方法 | 说明 |
|------|------|
| `empty()` | 创建空页表（IOMMU 用） |
| `root_paddr()` | 根节点物理地址 |
| `cursor(guard, va_range)` | 创建只读游标 |
| `cursor_mut(guard, va_range)` | 创建可写游标 |
| `new_kernel_page_table()` | 创建内核页表 |
| `create_user_page_table()` | 复制内核页表为用户页表 |

`VmSpace` — 用户虚拟地址空间：

| 方法 | 说明 |
|------|------|
| `new()` | 创建新地址空间（共享内核映射） |
| `cursor(guard, va_range)` | 只读游标查询 VA 范围 |
| `cursor_mut(guard, va_range)` | 可写游标修改 VA 范围 |
| `activate(self: &Arc<Self>)` | 激活页表（写 CR3/satp） |
| `reader(vaddr, len)` | 创建用户空间内存 Reader |
| `writer(vaddr, len)` | 创建用户空间内存 Writer |

`CursorMut` — 可写页表游标：

| 方法 | 说明 |
|------|------|
| `map(frame, prop)` | 映射物理页帧 |
| `map_iomem(io_mem, prop, len, offset)` | 映射 I/O 内存 |
| `unmap(len)` | 取消映射 |
| `protect_next(len, op)` | 修改页属性 |

#### task — 任务管理

| 类型 | 说明 |
|------|------|
| `Task` | 执行单元（入口函数 + 上下文 + 内核栈 + 调度信息） |
| `TaskOptions` | Builder 模式创建 Task |
| `CurrentTask` | 当前任务的 `!Send` 句柄 |

**Task 方法**：

| 方法 | 说明 |
|------|------|
| `Task::current() -> Option<CurrentTask>` | 获取当前任务 |
| `Task::yield_now()` | 让出 CPU |
| `Task::run(self: &Arc<Self>)` | 提交到调度器 |
| `Task::data() -> &Box<dyn Any>` | 获取任务共享数据 |

**TaskOptions Builder**：

| 方法 | 说明 |
|------|------|
| `new(func)` | 创建选项 |
| `data(data)` | 设置共享数据 |
| `local_data(data)` | 设置本地数据 |
| `build()` | 构建 Task |
| `spawn()` | 构建并立即运行 |

**上下文切换流程**（`processor.rs`）：

```
switch_to_task(next):
  1. 检查 atomic mode（不能在睡眠态切换）
  2. 完成 RCU 宽限期
  3. 关本地 IRQ
  4. 调用 pre-schedule handler
  5. CAS 防止 double-switch
  6. 保存旧 task，设置新 CURRENT_TASK_PTR
  7. arch::context_switch (汇编，保存/恢复寄存器)
     → 恢复后执行 after_switching_to:
  8. 恢复旧 task 的 Arc 引用
  9. post-schedule handler (激活 VmSpace)
  10. 开 IRQ
```

#### sync — 同步原语

| 类型 | 策略 | 适用场景 |
|------|------|---------|
| `SpinLock<T, G>` | 忙等 | 短临界区 |
| `Mutex<T>` | 等待队列阻塞 | 长临界区（I/O、复杂操作） |
| `RwLock` | 读写锁（忙等） | 读多写少 |
| `RwMutex` | 读写互斥锁（阻塞） | 读多写少，持锁时间长 |
| `RwArc<T>` | Arc 内嵌读写锁 | 数据共享 + 读写保护 |
| `Rcu<T>` | Read-Copy-Update | 读极多写极少（页表节点回收） |
| `WaitQueue` | 等待队列 | 配合 Mutex 使用 |

**SpinLock 泛型守卫机制详解**：

内核里自旋锁的核心问题是：**持锁期间不能被切换到另一个也想拿这把锁的执行流**，否则就死锁了。有两种"其他执行流"会抢 CPU：
1. **其他任务**（通过调度器抢占）
2. **中断处理程序**（硬件触发，随时打断）

SpinLock 通过泛型参数 `G` 控制持锁时禁用什么：

| 模式 | 抢占 | 中断 | 对应硬件操作 |
|------|------|------|-------------|
| `PreemptDisabled`（默认） | 禁用 | 允许 | 设置 per-CPU 抢占计数器，调度器不切换任务 |
| `LocalIrqDisabled` | 禁用 | 禁用 | x86: `cli` 指令 / riscv: `csr_clear sstatus.SIE`，CPU 不响应中断 |

**`SpinLock<T, PreemptDisabled>`**（默认）：

```rust
let lock = SpinLock::new(data);  // 默认 G = PreemptDisabled
let guard = lock.lock();
// 此时：抢占被禁用，但中断仍可能触发
// 如果中断处理程序也尝试 lock.lock() → 死锁！
// 所以：只有当中断处理程序不会访问同一把锁时，才用这个模式
```

**`SpinLock<T, LocalIrqDisabled>`**：

```rust
let lock: SpinLock<Data, LocalIrqDisabled> = SpinLock::new(data);
let guard = lock.lock();
// 此时：抢占和中断都被禁用
// 中断处理程序无法运行，安全
// 代价：关中断会增加中断延迟，影响系统响应性
```

**`.disable_irq()` — 动态升级**：

有些场景下，大部分时候只需要禁抢占，但偶尔需要禁中断。不需要定义两个锁：

```rust
let lock = SpinLock::new(data);  // PreemptDisabled

// 普通场景：只禁抢占（更快，中断延迟低）
let guard = lock.lock();

// 关键场景：需要禁中断（在中断处理程序也用到这把锁的代码路径）
let guard = lock.disable_irq().lock();
```

`.disable_irq()` 的实现原理：

```rust
// SpinLock<T, PreemptDisabled> 上的方法
pub fn disable_irq(&self) -> &SpinLock<T, LocalIrqDisabled> {
    // 把 *const SpinLock<T, PreemptDisabled>
    // 强转为 *const SpinLock<T, LocalIrqDisabled>
    // 因为 #[repr(transparent)]，两者的内存布局完全一样
    // （只有一个 AtomicBool + UnsafeCell<T>，PhantomData<G> 零大小）
    unsafe { &*(self as *const _ as *const _) }
}
```

它返回的是**同一个锁的不同类型视图**。之后调用 `.lock()` 时走的是 `LocalIrqDisabled` 的 `guard()` → `disable_local()`（关中断），而不是 `PreemptDisabled` 的 `guard()` → `disable_preempt()`（只禁抢占）。

**选择指南**：

| 场景 | 选择 |
|------|------|
| 锁不会被中断处理程序访问 | `PreemptDisabled`（默认，更快） |
| 锁**可能**被中断处理程序访问 | `LocalIrqDisabled` |
| 大部分路径不需要禁中断，少数路径需要 | 默认 + `.disable_irq()` 动态升级 |

**第三种守卫 `WriteIrqDisabled`**（用于 RwLock）：

```rust
// 读锁时只禁抢占，写锁时禁中断
// 前提：中断处理程序只会获取读锁，不会获取写锁
pub enum WriteIrqDisabled {}
impl SpinGuardian for WriteIrqDisabled {
    type Guard = DisabledLocalIrqGuard;     // 写锁：禁中断
    type ReadGuard = DisabledPreemptGuard;  // 读锁：禁抢占
}
```

`SpinLock<T>` 方法：

| 方法 | 说明 |
|------|------|
| `new(val)` | 创建自旋锁 |
| `lock()` | 获取锁（忙等，根据 `G` 自动禁用抢占或中断） |
| `try_lock()` | 非阻塞尝试获取 |
| `get_mut(&mut self)` | 零开销独占访问 |
| `disable_irq()` | 从 `PreemptDisabled` 动态升级为 `LocalIrqDisabled`（返回同一锁的不同类型视图） |

`Mutex<T>` 方法：

| 方法 | 说明 |
|------|------|
| `new(val)` | 创建互斥锁 |
| `lock()` | 阻塞直到获取（使用 WaitQueue） |
| `try_lock()` | 非阻塞尝试 |

#### irq — 中断处理

```
中断发生 → arch trap 入口
  → call_irq_callback_functions(trap_frame, irq_line, cpu_priv)
    → InterruptLevel::current() 从 Level 0 升到 Level 1
    → top_half: 调用已注册的回调函数
    → bottom_half: L1/L2 延迟处理
    → InterruptLevel 恢复
```

**中断嵌套深度**：

| 级别 | 说明 |
|------|------|
| Level 0 | 普通任务上下文（可抢占） |
| Level 1 | 中断上下文（本地 IRQ 关闭） |
| Level 2 | 嵌套中断（所有 IRQ 关闭，最大深度） |

**公开类型**：

| 类型 | 说明 |
|------|------|
| `IrqLine` | 硬件中断线 |
| `DisabledLocalIrqGuard` | RAII 关中断守卫 |
| `disable_local()` | 禁用本地 IRQ |

#### 其他模块

| 模块 | 说明 |
|------|------|
| `boot/` | 启动：内存区域、SMP |
| `bus/` | 总线抽象 |
| `console/` | 控制台（UART NS16550A） |
| `cpu/` | CPU 抽象（ID、per-CPU 变量） |
| `io/` | I/O 抽象（I/O 端口、内存映射 I/O） |
| `log/` | 日志系统（宏: `debug!`..`emerg!`） |
| `timer/` | 定时器（jiffies） |
| `smp/` | 多核 SMP |
| `power/` | 电源管理（关机/重启） |
| `panic/` | panic 处理 |
| `coverage/` | 代码覆盖率 |
| `user/` | 用户空间安全访问 |
| `ex_table/` | 异常表 |
| `prelude/` | 预导出（日志宏等） |
| `util/` | 工具（id_set, range_alloc 等） |

### 4.3 架构特定代码

每个架构实现统一的接口：

| 模块 | x86_64 | riscv64 | loongarch64 |
|------|--------|---------|-------------|
| boot | multiboot/multiboot2/linux | SBI | EFI |
| cpu | CPUID, MSRs, TDX | SBI 扩展 | — |
| irq | PIC + IOAPIC | PLIC | EIOINTC |
| mm | 4级页表 (4K/2M/1G) | SV39/SV48 | 4级页表 |
| timer | APIC/HPET/PIT | SBI timer | — |
| trap | GDT/IDT/syscall | STVEC/ECALL | — |
| task | System V AMD64 ABI | RISC-V ABI | LoongArch ABI |
| iommu | VT-d (DMA+中断重映射) | — | — |

### 4.4 附带库

| 库 | 作用 |
|------|------|
| `ostd-macros` | 过程宏：`#[ostd::main]`、`#[global_frame_allocator]`、`#[global_heap_allocator]`、`#[ktest]` |
| `ostd-test` | 测试框架数据模型：`KtestItem`、`KtestIter`（遍历 `.ktest_array` 段） |
| `linux-bzimage` | bzImage 构建（host 端工具），包含 boot-params、builder、setup |
| `ostd-pod` | 纯数据类型（POD），安全的位转换 |
| `align_ext` | 对齐扩展工具 |
| `id-alloc` | ID 分配器 |
| `int-to-c-enum` | 整数与 C 枚举互转 |
| `padding-struct` | 结构体内存布局填充 |

---

## 5. kernel — 安全 Rust 内核

**定位**：基于 ostd 构建的完整 Unix 兼容内核，**禁止 unsafe**。

### 5.1 入口

```rust
#[controlled]
#[ostd::main]
fn main() {
    init::main();  // 整个内核的启动序列
}
```

### 5.2 process — 进程管理

```
Process (struct)
  ├── pid: Pid (u32)
  ├── Arc<Vmar>                    ← 虚拟地址空间
  ├── TaskSet                      ← 线程集合
  ├── parent/children              ← 进程树
  ├── ProcessGroup / Session       ← 作业控制
  ├── sig_dispositions / sig_queues ← 信号
  ├── resource_limits              ← 资源限制
  ├── cgroup (RcuOption)           ← cgroup
  ├── user_ns                      ← 用户命名空间
  ├── timer_manager                ← 定时器
  └── nice / oom_score_adj         ← 调度和 OOM
```

**关键操作**：

| 函数/方法 | 说明 |
|-----------|------|
| `spawn_init_process()` | 创建 PID 1 进程 |
| `Process::current()` | 获取当前进程 |
| `Process::pid()` | 获取 PID |
| `Process::lock_vmar()` | 获取 VMAR 锁 |
| `Process::sig_dispositions()` | 信号处理表 |
| `Process::enqueue_signal()` | 入队信号 |
| `Process::stop/resume()` | 停止/恢复进程 |
| `Process::children()` | 子进程集合 |

**线程模型**：

| 类型 | 说明 |
|------|------|
| `PosixThread` | POSIX 线程（tid、affinity、robust_list） |
| `Thread` | 内核线程（包装 ostd Task + 信号架构处理） |

**进程间关系**：

| 类型 | 说明 |
|------|------|
| `ProcessGroup` | 进程组 |
| `Session` | 会话 |
| `Terminal` (trait) | 控制终端 |
| `JobControl` | 作业控制 |
| `ParentProcess` | 父进程（Weak 引用 + 缓存 PID） |

### 5.3 fs — 文件系统

#### VFS 层

```
VFS 层 (fs/vfs/)
  ├── inode/        — 核心 inode 抽象
  ├── file_system/  — 文件系统类型注册和挂载
  ├── path/         — 路径解析 (FsPath, PathResolver)
  ├── page_cache/   — 页缓存子系统
  ├── range_lock/   — POSIX 字节范围锁
  └── notify/       — 文件通知 (inotify/dnotify)
```

#### 文件系统实现

| 文件系统 | 说明 |
|----------|------|
| `ext2` | ext2 文件系统 |
| `exfat` | exFAT 文件系统 |
| `ramfs` | 内存文件系统 |
| `tmpfs` | tmpfs（基于 ramfs） |
| `procfs` | /proc（pid/, sys/kernel/） |
| `sysfs` | /sys |
| `devpts` | /dev/pts |
| `cgroupfs` | cgroup 文件系统（含 controller） |
| `configfs` | configfs |
| `overlayfs` | 联合挂载 |

#### 其他

| 模块 | 说明 |
|------|------|
| `pipe/` | 管道 |
| `file/` | 文件抽象（属性、inode 属性） |
| `utils/` | 文件系统工具 |

### 5.4 vm — 虚拟内存

#### Vmar (Virtual Memory Address Region)

```
Vmar (struct)
  ├── 包装 ostd::mm::VmSpace
  ├── IntervalSet<VmMapping>     ← 管理所有映射（区间树）
  ├── per-CPU RSS 计数器
  └── 子模块:
      ├── map/       mmap() 实现
      ├── unmap/     munmap() 实现
      ├── protect/   mprotect() 实现
      ├── remap/     mremap() 实现
      ├── page_fault/ 缺页处理（按需分配）
      ├── query/     查询映射 (供 /proc/pid/maps 使用)
      ├── fork/      fork 时的 COW 复制
      └── access_alien/ 跨进程内存访问
```

**Vmar 常量**：
- `VMAR_LOWEST_ADDR = 0x001_0000` (64 KiB)
- `VMAR_CAP_ADDR = MAX_USERSPACE_VADDR`

**Vmar 方法**：

| 方法 | 说明 |
|------|------|
| `new(process_vm) -> Arc<Vmar>` | 创建新 VMAR |
| `vm_space() -> &Arc<VmSpace>` | 返回 ostd VmSpace |
| `get_rss_counter(rss_type)` | RSS 计数（File/Anon） |
| `get_mappings_total_size()` | 总映射大小 |

#### Vmo (Virtual Memory Object)

```
Vmo (struct)
  ├── XArray<UFrame>             ← 稀疏页存储
  ├── Option<Arc<dyn Pager>>     ← 文件后备（page cache）
  ├── VmoFlags: RESIZABLE / CONTIGUOUS / DMA
  └── WritableMappingStatus      ← 可写映射计数
```

**Vmo 方法**：

| 方法 | 说明 |
|------|------|
| `commit_on(page_idx, flags)` | 提交（分配/获取）一页 |
| `try_commit_page(offset)` | 非阻塞提交 |
| `decommit(range)` | 释放页面范围 |
| `read(offset, writer)` | 读取到 VmWriter |
| `write(offset, reader)` | 从 VmReader 写入 |
| `clear(range)` | 清零范围 |
| `size()` / `resize(new_size)` | 大小管理 |

**Pager trait**（文件后备 VMO 的页面提供者）：

| 方法 | 说明 |
|------|------|
| `commit_page(idx)` | 提供一个页帧 |
| `update_page(idx)` | 页被修改（脏页） |
| `decommit_page(idx)` | 页不再需要 |
| `commit_overwrite(idx)` | 提供页帧（将被完全覆盖） |

#### Vmar 与 Vmo 的关系

```
Process
  └── Vmar (地址空间)
       ├── VmMapping: [0x400000-0x401000] → Vmo A (匿名, COW)
       ├── VmMapping: [0x7f000-0x8f000]   → Vmo B (文件后备, ext2 inode)
       └── VmMapping: [stack 区域]         → Vmo C (匿名, 可扩展)
```

### 5.5 syscall — 系统调用

约 150 个 syscall，通过宏生成分发函数。

**分发流程**：

```
用户态 syscall 指令
  → arch trap 入口
  → handle_syscall(ctx, user_ctx)
    → 提取 syscall 号 + 6 个参数 (SyscallArgument)
    → syscall_dispatch()  (宏生成的 match)
      → SYS_read   => sys_read(args[..3])
      → SYS_write  => sys_write(args[..3])
      → SYS_mmap   => sys_mmap(args[..6])
      → SYS_clone  => sys_clone(args[..5])
      → ...
    → SyscallReturn::Return(value) 或错误码
  → 写回 user_ctx.rax
```

**SyscallReturn 类型**：

| 变体 | 说明 |
|------|------|
| `Return(isize)` | 正常返回，设置 rax |
| `NoReturn` | 不返回（如 exit） |

### 5.6 net — 网络栈

```
net/
  ├── iface/          网络接口管理
  └── socket/         Socket 实现
       ├── ip/stream/     TCP
       ├── ip/datagram/   UDP
       ├── unix/stream/   Unix 域流
       ├── unix/datagram/ Unix 域数据报
       ├── netlink/       Netlink (route/kobject_uevent)
       ├── vsock/         VSOCK
       ├── options/       Socket 选项
       └── util/          网络地址工具
```

底层使用 `aster-bigtcp`（基于 smoltcp 的 TCP/IP 协议栈）。

### 5.7 device — 设备管理

```
Device (trait)
  ├── type_() → Char / Block
  ├── id() → DeviceId
  └── open() → Box<dyn FileIo>
```

| 设备 | 说明 |
|------|------|
| `tty/` | TTY + 虚拟终端 + 键盘 |
| `pty/` | 伪终端 (master/slave) |
| `evdev/` | 事件设备 |
| `fb/` | 帧缓冲 |
| `mem/` | /dev/null, /dev/zero, /dev/random 等 |
| `registry/` | 字符/块设备注册表 |
| `misc/` | 杂项设备 (tdxguest) |

### 5.8 sched — 调度器

| 类型 | 说明 |
|------|------|
| `SchedPolicy` | NORMAL / BATCH / IDLE / FIFO / RR |
| `Nice` | -20 (最高优先) ~ 19 (最低优先) |
| `RealTimePriority` | 1-99 |
| `SchedAttr` | 完整调度属性 |
| `loadavg()` | 系统负载 (1/5/15 分钟) |

### 5.9 ipc — 进程间通信

| 类型 | 说明 |
|------|------|
| `ipc/semaphore/posix/` | POSIX 信号量 |
| `ipc/semaphore/system_v/` | System V 信号量 |
| `IpcPermission` | IPC 权限（key, uid/gid, mode） |
| `IpcControlCmd` | IPC 控制命令 |

### 5.10 comps — 内核组件

| 组件 | 作用 |
|------|------|
| `block` | 块设备驱动框架 |
| `network` | 网络设备驱动框架 |
| `pci` | PCI 总线枚举和驱动 |
| `virtio` | Virtio 设备（块、网、控制台、GPU 等） |
| `console` | 控制台抽象 |
| `framebuffer` | 帧缓冲设备 |
| `input` | 输入设备 |
| `i8042` | i8042 键盘控制器 |
| `logger` | 日志组件 |
| `mlsdisk` | MLS 磁盘 |
| `softirq` | 软中断 |
| `time` | 时间子系统 |
| `uart` | UART 驱动 |
| `cmdline` | 内核命令行解析 |
| `systree` | 系统设备树 |

### 5.11 libs — 内核库

| 库 | 作用 |
|------|------|
| `aster-bigtcp` | TCP/IP 协议栈（基于 smoltcp） |
| `aster-util` | 通用工具 |
| `comp-sys` | 组件系统 |
| `device-id` | 设备 ID 管理 |
| `xarray` | XArray 数据结构（稀疏数组） |
| `aster-rights` | 权限管理 |
| `aster-rights-proc` | 权限过程宏 |
| `atomic-integer-wrapper` | 原子整数包装 |
| `cpio-decoder` | CPIO 归档解码（解压 initramfs） |
| `jhash` | Jenkins 哈希 |
| `keyable-arc` | 可作为 HashMap key 的 Arc |
| `logo-ascii-art` | ASCII Logo |
| `typeflags` | 类型标志 |
| `typeflags-util` | 类型标志工具 |

---

## 6. osdk — 开发工具与内核空间库

### 6.1 cargo-osdk CLI 工具

主机端工具，不在 kernel workspace 中。

```
osdk/src/
  ├── main.rs              入口
  ├── cli.rs               CLI 定义 (clap)
  ├── arch.rs              架构枚举
  ├── base_crate/          生成临时 base crate
  │   ├── *.template       Cargo.toml/main.rs/链接脚本模板
  │   └── mod.rs
  ├── bundle/              构建产物打包
  │   ├── bin.rs           内核二进制处理
  │   ├── vm_image.rs      VM 镜像 (ISO/QCOW2)
  │   └── file.rs          文件操作
  ├── commands/            子命令实现
  │   ├── build/           构建 (ELF → ISO/QCOW2/bzImage)
  │   │   ├── grub.rs      GRUB 镜像生成
  │   │   ├── qcow2.rs     QCOW2 转换
  │   │   └── bin.rs       二进制处理
  │   ├── run.rs           运行 (QEMU)
  │   ├── test.rs          测试
  │   ├── debug.rs         GDB 调试
  │   ├── profile.rs       火焰图性能分析
  │   ├── new/             新建项目 (模板)
  │   └── util.rs
  └── config/              配置系统
      ├── manifest.rs      OSDK.toml 解析
      ├── scheme/          Scheme 配置
      └── unix_args.rs     QEMU 参数解析
```

### 6.2 frame-allocator — 物理页帧分配器

基于 buddy system 的全局物理内存页帧分配器。

```rust
FrameAllocator (实现 GlobalFrameAllocator trait)
  ├── alloc(layout) → Paddr    ← 分配物理页帧（关 IRQ）
  ├── dealloc(addr, size)      ← 释放
  └── add_free_memory(addr, size) ← 启动时注册空闲区域
```

**内部结构**：

| 模块 | 说明 |
|------|------|
| `cache/` | per-CPU 缓存层 |
| `chunk/` | 连续页块 |
| `pools/` | buddy system 分配池（负载均衡） |
| `smp_counter/` | SMP 安全的空闲内存计数器 |

**注册方式**：`#[global_frame_allocator]` → 生成 `__GLOBAL_FRAME_ALLOCATOR_REF` 符号 → OSTD 启动时查找。

### 6.3 heap-allocator — 内核堆分配器

```rust
HeapAllocator (实现 GlobalHeapAllocator trait)
  ├── Slab 缓存 (小对象, 固定大小)
  └── 大槽分配 (大于 slab 的对象, 基于页)

CpuLocalBox<T>   ← per-CPU 分配，避免竞争
alloc_cpu_local() ← CPU 本地分配
```

**注册方式**：`#[global_heap_allocator]` + `#[global_heap_allocator_slot_map]`。

### 6.4 test-kernel — 内核态测试运行器

```
入口: #[ostd::ktest::main]
  → run_ktests()
    → KtestIter::new()  (遍历 .ktest_array 段)
    → 按 crate/module 组织测试树
    → 逐个执行 (catch_unwind 捕获 panic)
    → 支持白名单过滤 (KTEST_TEST_WHITELIST, KTEST_CRATE_WHITELIST)
    → 彩色输出 (passed/failed/filtered)
    → 全部通过 → ExitCode::Success
    → 有失败 → ExitCode::Failure
```

---

## 7. 三者关系总结

```
┌─────────────────────────────────────────────────────────┐
│  osdk (主机端工具)                                       │
│  ├── cargo-osdk CLI: 构建/运行/测试/调试/性能分析         │
│  ├── frame-allocator: buddy system 物理页帧分配           │
│  ├── heap-allocator: slab + 大槽 内核堆分配              │
│  └── test-kernel: #[ktest] 测试运行器                    │
├─────────────────────────────────────────────────────────┤
│  kernel (安全 Rust, #![deny(unsafe_code)])               │
│  ├── syscall: ~150 个 Linux 系统调用                     │
│  ├── process: 进程/线程/信号/会话/进程组                  │
│  ├── fs: VFS + ext2/exfat/ramfs/procfs/sysfs/...        │
│  ├── vm: Vmar + Vmo (地址空间管理 + COW + 按需分配)       │
│  ├── net: TCP/UDP/Unix/Netlink/VSOCK socket             │
│  ├── device: TTY/PTY/evdev/fb/mem 设备                  │
│  ├── ipc: POSIX + System V 信号量                        │
│  ├── sched: 调度策略 (NORMAL/FIFO/RR/BATCH/IDLE)         │
│  └── comps: block/network/pci/virtio 驱动组件            │
├─────────────────────────────────────────────────────────┤
│  ostd (唯一允许 unsafe)                                  │
│  ├── mm: 页表/VmSpace/Frame/堆/DMA/TLB/物理内存          │
│  ├── task: Task/上下文切换/调度器                         │
│  ├── sync: SpinLock/Mutex/RwLock/RCU/WaitQueue           │
│  ├── irq: 上半部/下半部/中断嵌套                          │
│  ├── arch: x86_64/riscv64/loongarch64 硬件抽象           │
│  ├── boot: SMP 启动/内存区域                             │
│  ├── cpu: per-CPU 变量/CPU ID                           │
│  ├── io: I/O 端口/内存映射 I/O                           │
│  ├── console/serial: 串口控制台                          │
│  ├── timer: jiffies 定时器                               │
│  ├── log: 日志宏 (debug!..emerg!)                        │
│  ├── power: 关机/重启                                    │
│  └── libs: ostd-macros/ostd-test/linux-bzimage/...      │
└─────────────────────────────────────────────────────────┘
```

### 依赖关系

```
kernel → ostd (使用 Task, VmSpace, Frame, SpinLock 等)
kernel → osdk/deps/* (使用 frame-allocator, heap-allocator)
osdk (CLI) → ostd/libs/linux-bzimage (构建 bzImage)
osdk/deps/* → ostd (实现 GlobalFrameAllocator 等 trait)
```

### 数据流

```
OSDK.toml + CLI args → Config
    ↓
生成 base crate (桥接 Cargo 和裸机)
    ↓
cargo build → 内核 ELF (依赖 ostd + osdk/deps)
    ↓
boot.method 决定打包方式:
  grub-rescue-iso → grub.cfg + grub-mkrescue → ISO
  qemu-direct     → bzImage 或 原始 ELF
  grub-qcow2      → ISO → qemu-img convert → QCOW2
    ↓
启动 QEMU 加载镜像
```
