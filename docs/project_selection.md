# Asterinas 选题难度分级指南

本文按照实现难度、依赖风险、测试成本和评审范围，对 Asterinas
潜在贡献选题进行分级。Issue 或 TODO 中的优先级标签很有参考价值，
但它不完全等同于实现难度。一个高优先级子系统，如果需要新框架、
跨子系统语义或专门硬件验证，仍然不适合作为第一个贡献选题。

## 分级标准

本文使用以下标准评估选题难度。

- 代码范围：预计会改动多少个子系统。
- 依赖链：是否依赖尚未完成的基础设施。
- 语义风险：是否容易保持 Linux 兼容行为。
- 测试成本：是否可以用聚焦的测试验证行为。
- 评审成本：评审者是否需要理解架构级不变量。

## S 级：最适合入门

这些任务相对自包含，通常是系统调用 flag、缺失字段、单个 ioctl，
或局部数据结构优化。

| 选题 | 为什么适合入手 | 备注 |
|------|----------------|------|
| 支持 `rename` 的 `NOREPLACE` | VFS 局部行为，Linux 语义清楚。 | `EXCHANGE` 也较适合；`WHITEOUT` 可能需要更多文件系统支持。 |
| 支持 `getrandom` 的 `GRND_NONBLOCK` 和 `GRND_INSECURE` | 系统调用表面较小。 | 取决于当前随机源就绪状态模型。 |
| 支持 `O_TMPFILE` | 文件创建路径上的聚焦改动。 | 需要认真处理 VFS 和文件系统能力检查。 |
| 实现真实的 `access` 和 `faccessat` 检查 | 用户可见行为清楚。 | 适合理解权限模型。 |
| 增加 `/proc/meminfo` | 公共接口，容易手动验证和写测试。 | 可以先实现最小 Linux 兼容字段集。 |
| 补齐 `/proc/[pid]/status` 缺失字段 | 可以逐字段增量推进。 | 建议一个逻辑字段组一个补丁。 |
| 实现部分 TTY line discipline 输出 flag | 终端局部行为。 | 适合为输出转换增加聚焦测试。 |
| 补齐部分 `evdev` ioctl | 可以一个 ioctl 一个 ioctl 做。 | 需要明确兼容行为。 |
| 执行一个 `RLIMIT_*` 资源限制 | 每个限制可以独立拆分。 | `RLIMIT_NOFILE` 是较好的候选。 |
| 处理 TCP `MSG_NOSIGNAL` | socket 局部行为。 | 风险低于 TCP 状态机的大改动。 |
| 清理 `XArray` 空节点 | 局部数据结构优化。 | 需要覆盖 cursor 操作和清理行为的单元测试。 |
| 将简单 range counter 改为线段树 | 边界清楚的算法任务。 | 需要测试重叠、删除和空区间。 |

## A 级：适合做成中等规模 PR

这些任务适合在熟悉相关子系统后推进。如果范围控制得好，仍然适合做成
单主题 Pull Request。

| 选题 | 主要难点 | 建议拆分 |
|------|----------|----------|
| 增加 `/proc/<tid>` 线程信息 | 需要理解 procfs 的 task/thread 布局。 | 先加目录项，再逐个增加文件。 |
| 补齐 `/proc/[pid]/stat` 字段 | 需要匹配 Linux 字段语义。 | 按字段来源子系统分组。 |
| 实现 `FUTEX_WAKE_OP` | 并发正确性要求高。 | 增加 wake 和 operation 组合的回归测试。 |
| 完善 robust futex | 涉及退出时清理和竞态处理。 | 拆成注册、退出、唤醒行为。 |
| 执行更多 `RLIMIT_*` 限制 | 不同限制会触及不同子系统。 | 一个限制一个补丁。 |
| 完善 `mremap` | 涉及 VMA 移动和地址空间不变量。 | 从最小缺失语义开始。 |
| 在 x86-64 支持 `MAP_32BIT` | 地址空间分配策略。 | 保持架构特定行为隔离。 |
| 增加部分 `clone3` 支持 | 进程创建 ABI 较复杂。 | 拆分 `set_tid` 和 `cgroup` 支持。 |
| 实现 Unix socket `SCM_RIGHTS` | 文件描述符生命周期和凭证处理。 | 增加描述符传递测试。 |
| 完善部分 UDP socket 选项 | socket option 兼容性。 | 一组选项一个补丁。 |

## B 级：较难，但仍可拆分

这些任务会跨子系统，或需要较强的兼容性判断。更适合已经熟悉相关代码的
贡献者。

| 选题 | 为什么更难 |
|------|------------|
| IPv6 支持 | 会影响 socket 地址类型、路由、netlink、测试和用户态兼容。 |
| 多网卡和路由表 | 需要替换网络初始化和查找路径中的单设备假设。 |
| TCP socket 完整性 | 部分 TODO 较小，但控制消息、keepalive、shutdown 和状态迁移需要广泛测试。 |
| Netlink route 内核完善 | 网络工具依赖细微的 ACK、namespace 和 dump 语义。 |
| mount propagation | shared/private/slave/unbindable 传播规则复杂，会影响 namespace 行为。 |
| 负 dentry 缓存 | 是性能任务，但正确性依赖失效处理和文件系统变更。 |
| capability API 重构 | 权限检查分布在进程、文件系统、namespace 和系统调用路径。 |
| coredump 支持 | 需要 ELF dump、进程状态采集、权限、rlimit 和文件写入。 |
| `mlock` 和 `mlockall` | 涉及 VM、page fault、资源限制和页面驻留。 |
| Unix socket `SEQPACKET` | 需要实现协议语义，不只是增加一个 socket option。 |

## C 级：大型子系统项目

这些项目很重要，但不适合作为第一个任务。它们通常需要先做设计讨论，
并拆成 RFC 级别的阶段目标。

| 选题 | 为什么规模大 |
|------|--------------|
| LSM 框架和 YAMA | 需要 security hook、credential 集成、策略注册和生命周期设计。 |
| seccomp | 涉及系统调用分发、filter 评估、继承规则、同步和兼容行为。 |
| device mapper、`dm-crypt` 和 `dm-verity` | 需要块层映射、加密、verity 元数据和设备生命周期支持。 |
| `io_uring` | 需要 ring buffer ABI、异步执行、fd 集成、poll、内存 pinning 和取消语义。 |
| Network namespace | 影响 socket、设备、路由、netlink、procfs 和 namespace 切换。 |
| IPC namespace 和 cgroup namespace | 需要 namespace 框架工作，并接入被隔离资源。 |
| ext4 文件系统 | 即使只做只读支持，也是较大的独立文件系统工程。 |
| devtmpfs | 需要设备模型集成、节点生命周期和权限行为。 |
| `SCHED_DEADLINE` 和 load tracking | 会改变调度器核心行为，需要正确性和性能验证。 |
| NUMA 支持 | 影响启动发现、内存管理、调度和 `getcpu`。 |
| page cache 重构 | 价值高，但会影响文件系统和内存管理路径，风险也高。 |

## D 级：架构和硬件专项

这些是长期工作，验证成本很高，通常需要硬件、模拟器支持或架构相关调试。

| 选题 | 主要风险 |
|------|----------|
| AArch64 架构支持 | 需要启动、异常处理、页表、中断、上下文切换、系统调用入口和设备发现。 |
| 完善 RISC-V 架构支持 | 已有基础有帮助，但 FPU、IOMMU、RNG 等 TODO 都偏硬件专项。 |
| 完善 LoongArch 架构支持 | Tier 3 状态下，中断处理、SMP、FPU 上下文和定时器都有较大缺口。 |
| IOMMU 支持 | DMA 安全和设备隔离是正确性关键，且高度依赖架构。 |
| 多 PCIe segment group | 需要移除 ACPI 和 PCI 枚举中的单 segment 假设。 |
| virtio-blk multi-queue | 涉及块层并发和性能行为。 |
| TDX 环境中的 virtio-mmio 支持 | 需要理解机密计算环境，并具备专门测试条件。 |

## 推荐入门选题

如果目标是完成第一个或第二个实用贡献，优先考虑以下选题。

1. 支持 `rename` 的 `NOREPLACE`。
2. 支持 `getrandom` flags。
3. 支持 `O_TMPFILE`。
4. 实现真实的 `access` 和 `faccessat` 检查。
5. 增加 `/proc/meminfo`。
6. 补齐 `/proc/[pid]/status` 缺失字段。
7. 实现部分 TTY line discipline 输出 flag。
8. 补齐部分 `evdev` ioctl。
9. 执行一个 `RLIMIT_*` 限制，例如 `RLIMIT_NOFILE`。
10. 处理 TCP `MSG_NOSIGNAL`。

## 贡献策略建议

第一个 Pull Request 应尽量保持窄范围。一个好的入门补丁应只改变一个行为，
增加聚焦测试，并避免引入新的框架级抽象。对于更大的功能，可以先提交一个
准备性补丁，例如增加测试、记录当前行为，或移除一个小的硬编码假设。

选题时，优先选择可以表达成一个可观察 Linux 兼容行为的 issue 或 TODO。
除非目标明确是写 RFC，否则不要从“设计整个子系统”作为第一步开始。
