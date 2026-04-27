# Asterinas 开发路线图与选题分级

> 基于代码库中 TODO/FIXME 标注、Issue 追踪和架构分析，将所有待完成选题按难度分为五级。
> 每级内部按推荐程度排序。

---

## 目录

- [分级标准](#分级标准)
- [一级 — 入门级（几天可完成）](#一级--入门级几天可完成)
- [二级 — 较容易（1-2 周可完成）](#二级--较容易1-2-周可完成)
- [三级 — 中等难度（2-4 周可完成）](#三级--中等难度2-4-周可完成)
- [四级 — 较难（1-3 个月）](#四级--较难1-3-个月)
- [五级 — 最难（3 个月以上）](#五级--最难3-个月以上)
- [推荐路线](#推荐路线)

---

## 分级标准

| 级别 | 工作量 | 典型特征 |
|------|--------|----------|
| 一级 | 几天 | 范围小、独立、改动集中、TODO 明确 |
| 二级 | 1-2 周 | 需要理解子系统，但主要工作是补全已有框架中的缺失逻辑 |
| 三级 | 2-4 周 | 需要理解某个子系统的整体设计，有一定架构决策 |
| 四级 | 1-3 月 | 涉及新子系统、跨模块改动或重大架构决策 |
| 五级 | 3 月+ | 大规模架构工作或全新子系统 |

---

## 一级 — 入门级（几天可完成）

适合入门或快速产出，改动范围小且独立。

### 1. 支持 getrandom 的 GRND_NONBLOCK 和 GRND_INSECURE 标志

- **位置**: `kernel/src/syscall/getrandom.rs`（44 行）
- **现状**: 文件中有明确 TODO，两个 flag 已定义但未实现处理逻辑
- **工作量**: 半天
- **做法**: 在现有 getrandom 逻辑中根据 flag 位控制阻塞/非阻塞行为和安全性检查
- **验证方法**:
  1. 编写用户态 C 测试程序：分别以 `GRND_NONBLOCK` 和 `GRND_INSECURE` 调用 `getrandom()`，确认返回值和 errno 符合 Linux 行为
  2. 对比 Linux 上相同测试程序的输出，确保语义一致
  3. 运行 `osdk test` 确认无回归

### 2. 增加 O_TMPFILE 标志

- **位置**: `kernel/src/fs/file/file_attr/creation_flags.rs`（23 行）
- **现状**: bitflags 定义中缺少 `O_TMPFILE`
- **工作量**: 半天
- **做法**: 在 `CreationFlags` 中添加 `O_TMPFILE` 位定义，并在 VFS 创建路径中处理该标志
- **验证方法**:
  1. 在用户态编写测试：`open("/tmp", O_TMPFILE | O_RDWR, 0666)` 应成功返回 fd
  2. 验证创建的临时文件在目录列表中不可见（`readdir` 不应列出）
  3. 对该 fd 执行 `write` + `linkat` 使其变为可见文件
  4. 确认不传 `O_TMPFILE` 时原有行为不变

### 3. 支持 rename 的 NOREPLACE / EXCHANGE / WHITEOUT 标志

- **位置**: `kernel/src/syscall/rename.rs`（103 行）
- **现状**: 三个 flag 已定义，但当前遇到非零 flags 直接返回 `EINVAL`
- **工作量**: 1-2 天
- **做法**: 为每个 flag 添加分支逻辑——NOREPLACE 在目标存在时返回 EEXIST，EXCHANGE 交换两个路径，WHITEOUT 创建白out 设备
- **验证方法**:
  1. `renameat2(AT_FDCWD, "a", AT_FDCWD, "b", RENAME_NOREPLACE)`：目标不存在时成功；目标已存在时返回 `EEXIST`
  2. `renameat2(AT_FDCWD, "a", AT_FDCWD, "b", RENAME_EXCHANGE)`：两个文件内容互换
  3. 在 Linux 上运行相同测试确认行为一致
  4. 运行 `osdk test` 确认无回归

### 4. 支持 MAP_32BIT mmap 标志

- **位置**: `kernel/src/syscall/mmap.rs`（302 行，第 88-91 行）
- **现状**: 当前仅 `log::warn!` 打印警告后继续执行，未约束分配地址
- **工作量**: 1-2 天
- **做法**: 检测 `MAP_32BIT` 标志后，将分配地址约束在 2GiB 以下
- **验证方法**:
  1. 用户态测试：`mmap(NULL, 4096, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_32BIT, -1, 0)`
  2. 检查返回地址 `< 0x80000000`（2GiB）
  3. 不带 `MAP_32BIT` 时地址可高于 2GiB
  4. 对比 Linux 行为确认一致

### 5. 完善 access/faccessat — 去除 dummy 实现

- **位置**: `kernel/src/syscall/access.rs`（122 行，第 108 行）
- **现状**: 实现标记为 dummy，基础的 `check_permission` 框架已有
- **工作量**: 1-2 天
- **做法**: 补全真实的权限检查逻辑，包括基于 UID/GID 和文件 mode 的判断
- **验证方法**:
  1. 以 root 身份 `access("/root/file", R_OK)` 应返回 0
  2. 以普通用户 `access("/root/file", R_OK)` 应返回 `-1` 且 `errno == EACCES`
  3. 测试 `faccessat` 的 `AT_EACCESS` 标志（使用有效 UID 而非真实 UID）
  4. 对比 Linux 上相同测试的结果

### 6. 完善 statx 系统调用 — 填充虚拟字段

- **位置**: `kernel/src/syscall/statx.rs`（第 149、163 行）
- **现状**: 多个字段返回占位值，缺少 birth time 支持
- **工作量**: 1-2 天
- **做法**: 逐个补充真实数据来源（如 stx_btime、stx_attributes 等）
- **验证方法**:
  1. 编写用户态测试调用 `statx()` 并打印所有字段，与 Linux 上的输出逐字段对比
  2. 重点验证：`stx_btime`（文件创建时间）非零、`stx_blksize` 合理、`stx_nlink` 正确
  3. 对特殊文件（符号链接、设备节点、目录）也进行验证

### 7. 优化 segment tree 替换 range_counter

- **位置**: `ostd/src/util/range_counter.rs`（230 行，第 5 行）
- **现状**: 使用简单 BTreeMap 实现，TODO 明确建议"用线段树优化到 O(log(range size))"
- **工作量**: 1-2 天
- **做法**: 实现线段树替代现有 BTreeMap 方案
- **验证方法**:
  1. 运行 `range_counter.rs` 中已有的所有单元测试，确认全部通过
  2. 编写基准测试（benchmark）：分别用大量 insert/remove/query 操作对比优化前后耗时
  3. 确认 DMA 和 I/O 内存分配功能正常（这是该模块的实际使用者）
  4. 运行 `osdk test` 确认内核整体无回归

### 8. 优化 XArray 的空节点清理

- **位置**: `kernel/libs/xarray/src/cursor.rs`（第 470 行）
- **现状**: cursor 操作不清理空内部节点，导致内存膨胀
- **工作量**: 1-2 天
- **做法**: 在 cursor 操作返回时增加空节点回收逻辑
- **验证方法**:
  1. 运行 `kernel/libs/xarray/` 下的所有单元测试
  2. 编写压力测试：大量 insert 后逐个 remove，验证内存使用量回落（而非持续增长）
  3. 运行 `osdk test` 确认内核整体无回归

---

## 二级 — 较容易（1-2 周可完成）

有一定设计空间，但主要工作是补全已有框架中的缺失逻辑。

### 9. 增加 mlock / mlockall 系统调用

- **位置**: 新增系统调用
- **现状**: 完全未实现
- **工作量**: 3-5 天
- **做法**: 在 VMO 层增加 pin/unpin 逻辑，防止页面被回收；实现 `mlock`、`mlock2`、`munlock`、`mlockall`、`munlockall` 五个系统调用
- **验证方法**:
  1. 用户态测试：`mlock(addr, len)` 后检查返回值为 0
  2. 锁定内存后验证进程的 `RSS` 不会下降（对比 `/proc/self/status` 中 VmRSS）
  3. 测试 `mlockall(MCL_CURRENT)` 和 `mlockall(MCL_FUTURE)` 语义
  4. 测试超出 `RLIMIT_MEMLOCK` 限制时返回 `ENOMEM`
  5. `munlock` / `munlockall` 后验证页面可被回收
  6. 对比 Linux 行为确认一致

### 10. 完善 rlimit 资源限制执行

- **位置**: 多个文件
- **现状**: Issue #2841，多数 `RLIMIT_*` 常量定义了但未实际执行限制检查
- **工作量**: 每个子任务 1-3 天（共约 16 个子任务）
- **做法**: 在对应操作前添加限制检查（如 RLIMIT_NOFILE 在 open 时检查、RLIMIT_STACK 在栈扩展时检查等），可逐个认领
- **验证方法**（以 `RLIMIT_NOFILE` 为例）:
  1. `setrlimit(RLIMIT_NOFILE, {rlim_cur=10, rlim_max=10})` 设置限制
  2. 循环 `open()` 直到打开 10 个 fd，第 11 次应返回 `EMFILE`
  3. `getrlimit(RLIMIT_NOFILE, &rlim)` 验证读取值与设置值一致
  4. 每个 RLIMIT 子任务都有对应的 Linux 测试用例（可参考 LTP 的 rlimit 测试）
  5. 运行 `osdk test` 确认无回归

### 11. 完善 signal handling — 补全 siginfo_t 字段

- **位置**: `kernel/src/process/signal/` 及相关文件
- **现状**: Issue #2913，`si_timerid` 和 `si_overrun` 等 POSIX timer 信号字段缺失
- **工作量**: 3-5 天
- **做法**: 在 siginfo_t 结构体和信号投递逻辑中补充缺失字段
- **验证方法**:
  1. 编写测试程序：创建 POSIX timer（`timer_create`），设置超时后在信号处理函数中检查 `si->si_signo == SIGUSR1`、`si->si_code == SI_TIMER`、`si->si_timerid` 正确、`si->si_overrun` 合理
  2. 对比 Linux 上相同程序的 `si_timerid` 和 `si_overrun` 值
  3. 运行 `osdk test` 确认无回归

### 12. 实现细粒度日志过滤

- **位置**: 日志子系统
- **现状**: Issue #1503，已有详细设计方案
- **工作量**: 3-5 天
- **做法**: 按设计方案实现按模块设置日志级别的功能
- **验证方法**:
  1. 将某个模块的日志级别设为 `warn`，确认 `info!` 和 `debug!` 宏不再输出该模块日志
  2. 将全局级别设为 `error`，单独将某模块设为 `debug`，确认只有该模块输出 debug 级别日志
  3. 通过内核命令行参数（如 `log.level=warn,net=debug`）验证启动时配置生效
  4. 运行 `osdk test` 确认日志过滤不影响正常功能

### 13. 引入动态调试框架

- **位置**: 日志子系统
- **现状**: Issue #2941，类似 Linux 的 `dynamic_debug`
- **工作量**: 5-7 天
- **做法**: 实现运行时可配置的调试日志开关机制
- **验证方法**:
  1. 代码中使用 `pr_debug!("msg")` 标记调试语句
  2. 通过 sysfs 或命令行动态开关：`echo "module_name +p" > /sys/kernel/debug/dynamic_debug/control`
  3. 开启后确认对应 `pr_debug!` 输出可见，关闭后不再输出
  4. 运行 `osdk test` 确认无回归

### 14. 增加 membarrier 系统调用

- **位置**: 新增系统调用
- **现状**: 未实现，对高性能并发程序重要
- **工作量**: 3-5 天
- **做法**: 实现跨核内存屏障广播，ostd 层已有 IPI 基础设施可复用
- **验证方法**:
  1. 编写多线程测试：一个线程写数据后调用 `membarrier(MEMBARRIER_CMD_GLOBAL)`，另一个线程在 `rseq` 或自旋等待后应看到最新数据
  2. 测试各命令：`MEMBARRIER_CMD_GLOBAL`、`MEMBARRIER_CMD_GLOBAL_EXPEDITED`、`MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED`
  3. 在 SMP=2 或更高配置下运行验证（`osdk run --qemu-args="-smp 4"`）
  4. 对比 Linux `man membarrier` 文档中的语义说明

### 15. 完善 TTY line discipline

- **位置**: `kernel/src/device/tty/line_discipline.rs`
- **现状**: 第 160 行未实现输出标志处理，第 244 行 canonical 模式切换行为不正确
- **工作量**: 5-7 天
- **做法**: 实现输出标志处理（OPOST 等）并修正 canonical 模式切换逻辑
- **验证方法**:
  1. 在 Asterinas shell 中输入命令，验证回显和行编辑正常
  2. 测试 `OPOST` 标志：设置后换行符应自动添加 `\r`（`\n` → `\r\n`）
  3. 测试 canonical/non-canonical 模式切换：`stty -raw` / `stty raw` 后行为应改变
  4. 测试特殊字符：Ctrl+C（SIGINT）、Ctrl+D（EOF）、Backspace（删除）
  5. 对比 Linux 上相同终端操作的行为

### 16. 完善 evdev 事件设备 — 补全 ioctl

- **位置**: `kernel/src/device/evdev/file.rs`
- **现状**: 多个 ioctl 操作未实现，缺少设备节点删除功能（`evdev/mod.rs:284`）
- **工作量**: 5-7 天
- **做法**: 逐个实现缺失的 evdev ioctl 操作
- **验证方法**:
  1. 编写用户态程序：`open("/dev/input/event0", O_RDWR)` 后调用各种 `ioctl`（如 `EVIOCGVERSION`、`EVIOCGID`、`EVIOCGBIT`、`EVIOCGNAME` 等）
  2. 验证每个 ioctl 返回值与 Linux 行为一致
  3. 使用 `evtest` 工具（如已移植）读取设备事件
  4. 运行 `osdk test` 确认无回归

### 17. 优化单元测试执行速度

- **现状**: Issue #1904，单元测试是 CI 瓶颈
- **工作量**: 5-7 天
- **做法**: 通过并行化、增量测试、按模块分组等手段优化测试执行时间
- **验证方法**:
  1. 优化前记录 `time osdk test` 全量执行时间作为基线
  2. 优化后再次记录执行时间，确认有显著改善（目标：减少 30% 以上）
  3. 确认所有测试结果与优化前一致（无假通过/假失败）
  4. 验证增量测试模式：仅修改单个模块后只运行该模块相关测试

### 18. 全面启用 RISC-V CI 测试

- **现状**: Issue #2546，当前 RISC-V 测试覆盖有限
- **工作量**: 3-5 天
- **做法**: 扩展 CI 配置，增加 RISC-V 目标的测试用例
- **验证方法**:
  1. 提交 PR 后在 CI 中确认 RISC-V 构建和测试任务成功运行
  2. 对比 x86_64 CI 的测试覆盖范围，确保 RISC-V CI 覆盖了相同的核心测试集
  3. 确认 CI 执行时间在可接受范围内（不显著拖慢整体 CI）

### 19. 完善 clone3 的 set_tid / cgroup 支持

- **位置**: `kernel/src/syscall/clone.rs`（第 89 行）
- **现状**: 标记为 TODO
- **工作量**: 3-5 天
- **做法**: 在现有 clone3 实现基础上扩展 `set_tid` 和 cgroup 参数的处理
- **验证方法**:
  1. `clone3()` 时传入 `set_tid = [42]`，验证子进程 PID 为 42
  2. `clone3()` 时传入 `set_tid` 数组包含多个元素（多命名空间场景）
  3. 验证 cgroup 参数：子进程应被放入指定 cgroup
  4. 测试错误情况：指定已被占用的 PID 应返回 `EEXIST`
  5. 对比 Linux `man clone3` 行为

### 20. 用内联汇编替换 read_volatile / write_volatile

- **现状**: Issue #2948
- **工作量**: 3-5 天
- **做法**: 在关键路径中将 `read_volatile`/`write_volatile` 替换为内联汇编以获得更好的优化控制，需仔细验证语义正确性
- **验证方法**:
  1. 每替换一处后立即运行 `osdk test` 确认内核正常启动
  2. 对替换点编写专门的单元测试：验证读写值的正确性和顺序性
  3. 在 SMP 配置下运行，验证多核场景下 MMIO 操作不被重排
  4. 如果有基准测试，对比替换前后的性能差异

### 21. 让 frame allocator 回收 bootloader 内存区域

- **位置**: ostd 帧分配器
- **现状**: Issue #322，引导程序使用的内存在启动后未被回收
- **工作量**: 3-5 天
- **做法**: 在启动后期将 bootloader 占用的内存区域标记为可分配
- **验证方法**:
  1. 优化前记录可用物理内存总量（启动日志中查看）
  2. 实现后确认可用物理内存总量增加（说明 bootloader 内存被回收）
  3. 分配并使用回收区域中的页面，确认无异常（写入测试数据后读回验证）
  4. 运行 `osdk test` 确认无回归
  5. 启用 KASAN 等调试工具（如可用）检测越界访问

### 22. 修复单个 gvisor / LTP blocked 测试用例

- **位置**: `test/initramfs/src/conformance/`
- **现状**: gvisor 663 个测试被 block，LTP ext2 56 个、exFAT 103 个被 block，kselftest 112 个被 block
- **工作量**: 每个测试 1-5 天
- **做法**: 按子系统逐个分析失败原因并修复，每个测试对应一个具体的 bug 或缺失功能
- **验证方法**:
  1. 从 blocklist 中移除该测试用例
  2. 运行完整测试套件：`make run-tests` 或对应命令
  3. 确认该测试用例由 FAIL 变为 PASS
  4. 确认没有引入新的失败用例
  5. 提交 PR 时在描述中附上修复前后的测试结果对比

---

## 三级 — 中等难度（2-4 周可完成）

需要理解某个子系统的整体设计，有一定架构决策。

### 23. 增加 seccomp 子系统

- **优先级**: 高（容器安全关键组件）
- **现状**: 完全没有 seccomp 实现
- **工作量**: 2-3 周
- **做法**: 实现系统调用过滤机制（BPF 规则），包括 `seccomp` 系统调用和 `prctl(PR_SET_SECCOMP)` 入口，支持 strict 和 filter 两种模式
- **参考**: Linux 内核 seccomp 实现
- **验证方法**:
  1. **strict 模式测试**: 调用 `prctl(PR_SET_SECCOMP, SECCOMP_MODE_STRICT)` 后，只允许 `read`/`write`/`_exit`/`sigreturn`，其他系统调用应触发 `SIGKILL`
  2. **filter 模式测试**: 通过 `prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &prog)` 加载 BPF 规则，验证规则生效（如禁止 `openat` 后调用应返回 `EPERM` 或触发 `SIGSYS`）
  3. 编译并运行 Linux 的 `seccomp` 测试程序（如 `kernel/seccomp.c` 自测或 libseccomp 附带的测试）
  4. 验证 `SECCOMP_RET_TRAP`、`SECCOMP_RET_ERRNO`、`SECCOMP_RET_ALLOW`、`SECCOMP_RET_KILL` 四种返回行为
  5. 运行 gvisor 中 seccomp 相关测试用例

### 24. 增加 coredump 支持

- **位置**: `kernel/src/syscall/prctl.rs`（第 38-45 行）
- **现状**: `PR_GET_DUMPABLE` 始终返回 DISABLE，`PR_SET_DUMPABLE` 为空操作
- **工作量**: 2-3 周
- **做法**: 实现 ELF core dump 文件生成、信号触发机制、dumpable 状态管理
- **验证方法**:
  1. `prctl(PR_SET_DUMPABLE, 1)` 后 `prctl(PR_GET_DUMPABLE)` 应返回 1
  2. 编写触发段错误的程序（`*(int*)0 = 42`），确认内核生成 core 文件
  3. 使用 `readelf` 或 `gdb` 检查 core 文件格式：验证 ELF header 中 `e_type == ET_CORE`，`NT_PRSTATUS`、`NT_AUXV` 等 note 段存在且内容正确
  4. 用 `gdb ./a.out core` 加载 core dump，确认能还原崩溃时的寄存器状态和栈回溯
  5. 验证 `/proc/sys/kernel/core_pattern` 控制文件生效

### 25. 增加 IPC namespace

- **位置**: `kernel/src/process/namespace/nsproxy.rs`（第 94 行）
- **现状**: 标记为 TODO: "Support other namespaces"
- **工作量**: 2-3 周
- **做法**: 参照已有的 mount/uts/pid namespace 实现，为 SYSV IPC 资源（信号量、共享内存、消息队列）添加命名空间隔离
- **验证方法**:
  1. `unshare(CLONE_NEWIPC)` 后在子进程中创建信号量，确认父进程看不到该信号量（`ipcs` 输出不同）
  2. 在两个不同 IPC namespace 中使用相同 IPC key 创建信号量，应各自独立
  3. 验证 `/proc/sysvipc/*` 在不同 namespace 中显示不同内容
  4. 测试 `setns()` 切换到目标 IPC namespace 后能访问其中的 IPC 资源
  5. 运行 LTP 中 namespace 相关测试

### 26. 增加 Cgroup namespace

- **位置**: `kernel/src/process/namespace/nsproxy.rs`
- **现状**: 与 IPC namespace 一起标记为 TODO
- **工作量**: 2-3 周
- **做法**: 参照已有 namespace 实现模式，为 cgroup 添加虚拟化视图
- **验证方法**:
  1. `unshare(CLONE_NEWCGROUP)` 后查看 `/proc/self/cgroup`，确认 cgroup 路径显示为虚拟化视图（以 `/` 为根）
  2. 在 cgroup namespace 内部移动进程到子 cgroup，确认不影响外部视图
  3. 测试 `setns()` 切换 cgroup namespace
  4. 运行 LTP 中 cgroup namespace 测试

### 27. 完善 procfs — 增加缺失的 inodes

- **相关 Issue**: #946（/proc/meminfo）、#2940（/proc/\<tid\>）
- **现状**: `/proc/[pid]/status` 和 `/proc/[pid]/stat` 有 FIXME 标记的未实现字段（`status.rs:22`、`stat.rs:24`）
- **工作量**: 2-3 周
- **做法**: 每个 inode 独立，可分批实现。包括 meminfo、线程目录、status/stat 中缺失字段等
- **验证方法**:
  1. **/proc/meminfo**: 在 Asterinas 和 Linux 上分别 `cat /proc/meminfo`，逐字段对比（MemTotal、MemFree、Buffers、Cached 等），允许数量级差异但字段应齐全
  2. **/proc/[pid]/status**: 检查 Name、State、Pid、PPid、Uid、Gid、VmSize、VmRSS 等字段的正确性
  3. **/proc/[pid]/stat**: 使用 `ps aux` 验证能正确显示进程信息
  4. **/proc/[tid]/**: 多线程程序中检查每个线程是否都有对应目录和正确的 status/stat
  5. 运行 gvisor proc_test 中相关的 blocked 测试

### 28. 增加 devtmpfs

- **相关 Issue**: #1990
- **现状**: `/dev` 下使用临时方案
- **工作量**: 2-3 周
- **做法**: 实现内核自动管理的 tmpfs，设备驱动注册/注销时自动创建/删除设备节点
- **验证方法**:
  1. 启动后 `ls -la /dev/` 应显示自动创建的设备节点（console、null、zero、random、tty 等）
  2. 加载新设备驱动后，确认 `/dev` 下自动出现对应设备节点
  3. 卸载设备驱动后，确认 `/dev` 下对应节点自动消失
  4. `mknod` 创建的设备节点应正常工作
  5. 对比 Linux 上 `devtmpfs` 的行为

### 29. 完善 capability 检查机制

- **相关 Issue**: #2381
- **位置**: `kernel/src/process/credentials/mod.rs`（第 32 行）
- **现状**: 需要重新设计 capability 检查 API，支持 bounding 和 ambient capability set
- **工作量**: 2-3 周
- **做法**: 扩展 credentials 结构体，实现 bounding/ambient/inheritable 四种 capability set，并重构权限检查逻辑
- **验证方法**:
  1. `capset()` 设置 ambient capability 后，`execve()` 新程序应继承该 capability
  2. bounding set 限制：设置 bounding set 后 `capset()` 无法提升到 bounding set 之外的 capability
  3. 以普通用户执行需要 `CAP_NET_BIND_SERVICE` 的操作（绑定低端口号），设置该 capability 后应成功
  4. 运行 LTP 中 capability 相关测试
  5. 运行 `osdk test` 确认无回归

### 30. 完善 futex — 增加 FUTEX_WAKE_OP 等

- **位置**: `kernel/src/process/posix_thread/futex.rs`
- **现状**: 缺少 `FUTEX_WAKE_OP` 操作，robust futex 支持不完善，第 128 行有竞态条件 FIXME
- **工作量**: 2-3 周
- **做法**: 实现 `FUTEX_WAKE_OP`（原子用户态操作 + 唤醒）、完善 robust list 处理、修复竞态条件
- **验证方法**:
  1. 编写多线程测试使用 `FUTEX_WAKE_OP`：线程 A 等待 futex，线程 B 通过 WAKE_OP 原子修改并唤醒
  2. 测试 robust futex：线程持有 robust mutex 时被 kill，内核应自动清理并唤醒等待者（设置 `FUTEX_OWNER_DIED`）
  3. 使用 `perf` 或自定义 benchmark 验证无性能退化
  4. 运行 gvisor 中 futex 相关测试
  5. 运行 `osdk test` 确认无回归

### 31. 增加 /dev/random 真随机性支持

- **位置**: `kernel/src/device/mem/file.rs`（第 42 行）
- **现状**: 标记为 TODO，需要收集环境噪声
- **工作量**: 2 周
- **做法**: 建立内核熵池，从硬件（RDRAND/RDSEED）和软件（定时器抖动、中断时序）收集熵，实现 CSPRNG
- **验证方法**:
  1. 从 `/dev/random` 和 `/dev/urandom` 分别读取数据，验证非全零且不重复
  2. 使用统计测试工具（如 `rngtest` 或 `dieharder`）检验随机性质量
  3. `/dev/random` 在熵不足时应阻塞（`poll` 返回无数据可读），`/dev/urandom` 不应阻塞
  4. 写入 `/dev/urandom` 应增加熵池熵值（影响 `/dev/random` 的阻塞行为）
  5. 对比 Linux 的行为差异文档

### 32. 完善 setns 系统调用

- **位置**: `kernel/src/syscall/setns.rs`（第 107、146、213、243 行多处 TODO）
- **现状**: 命名空间切换不完整，权限检查不充分
- **工作量**: 2-3 周
- **做法**: 补全其他 namespace 的切换逻辑，完善权限检查
- **验证方法**:
  1. 通过 `setns(fd, CLONE_NEWNET)` 切换 network namespace，验证 `/proc/self/ns/net` 链接目标改变
  2. 测试 `setns(fd, 0)` 自动检测 namespace 类型的行为
  3. 以非特权用户调用 `setns` 应返回 `EPERM`（除非有 `CAP_SYS_ADMIN`）
  4. 验证多线程进程中 `setns` 只影响调用线程
  5. 运行 LTP 中 setns 相关测试

### 33. 完善 mremap 系统调用

- **位置**: `kernel/src/syscall/mremap.rs`（第 81、94 行 FIXME/TODO）
- **工作量**: 2 周
- **做法**: 完善虚拟内存重映射的页表操作，处理 MREMAP_MAYMOVE 和 MREMAP_FIXED 标志
- **验证方法**:
  1. `mremap(ptr, old_size, new_size, 0)` 原地扩展，验证返回地址不变且数据完整
  2. `mremap(ptr, old_size, new_size, MREMAP_MAYMOVE)` 允许移动，验证数据完整
  3. `mremap(ptr, old_size, new_size, MREMAP_FIXED | MREMAP_MAYMOVE, new_addr)` 移动到指定地址
  4. 缩小映射：验证截断部分不再可访问（访问应触发 SIGSEGV）
  5. 对比 Linux `man mremap` 文档中各场景

### 34. 完善 TCP socket 实现

- **位置**: `kernel/src/net/socket/ip/stream/mod.rs`
- **现状**: 多处 TODO：shutdown listening stream（第 504 行）、MSG_NOSIGNAL 处理（第 567 行）、控制消息收发（第 561、582 行）、keep-alive idle 跟踪（第 750 行）等
- **工作量**: 2-3 周
- **做法**: 逐个修复 TODO，可分批提交
- **验证方法**:
  1. **shutdown**: `shutdown(sockfd, SHUT_WR)` 后 `write` 应返回 `EPIPE`，对端 `read` 应返回 0（EOF）
  2. **MSG_NOSIGNAL**: 设置后对已关闭的 socket 写入不应产生 `SIGPIPE`
  3. **keep-alive**: `setsockopt(SO_KEEPALIVE)` + `setsockopt(TCP_KEEPIDLE, ...)` 后，空闲超时后应发送 keep-alive 探测包
  4. 运行 gvisor 中 `tcp_socket_test` 被 blocked 的测试
  5. 使用 packetdrill（如果已集成）验证 TCP 状态机转换

### 35. 完善 UDP socket 实现

- **位置**: `kernel/src/net/socket/ip/datagram/mod.rs`（第 50-244 行多处 TODO）
- **工作量**: 2 周
- **做法**: 补全 socket 选项设置和多种操作
- **验证方法**:
  1. 测试 `setsockopt` / `getsockopt` 的 `SO_RCVBUF`、`SO_SNDBUF`、`SO_REUSEADDR`、`IP_MULTICAST_TTL` 等选项
  2. 测试 `recvmsg` / `sendmsg` 的 `MSG_DONTWAIT` 标志
  3. 测试 `connect()` 后的 UDP socket 只能与指定对端通信
  4. 运行 gvisor 中 UDP 相关 blocked 测试
  5. 运行 `osdk test` 确认无回归

### 36. 完善 Unix domain socket — 控制消息

- **位置**: `kernel/src/net/socket/unix/ctrl_msg.rs`
- **现状**: SCM_RIGHTS 和 SCM_CREDENTIALS 实现不完整，SEQPACKET 类型缺失
- **工作量**: 2-3 周
- **做法**: 实现 fd 传递（SCM_RIGHTS）、凭证传递（SCM_CREDENTIALS）和 SEQPACKET 类型
- **验证方法**:
  1. **SCM_RIGHTS**: 进程 A 通过 Unix socket 发送 fd，进程 B `recvmsg` 后用该 fd 读写文件，验证数据一致
  2. **SCM_CREDENTIALS**: `recvmsg` 收到 `struct ucred`（pid、uid、gid），验证值正确
  3. **SEQPACKET**: 创建 `SOCK_SEQPACKET` 类型 socket，验证消息边界保留（发送两条消息，接收方分别收到完整的两条而非合并的一条）
  4. 运行 gvisor 中 Unix socket 相关测试

### 37. 增加多 TTY 虚拟终端

- **相关 Issue**: #2820
- **工作量**: 2-3 周
- **做法**: 实现 Ctrl+Alt+Fn 虚拟终端切换，需要终端管理和键盘事件路由
- **验证方法**:
  1. 启动后 `Ctrl+Alt+F2` 切换到 tty2，确认屏幕显示新的登录提示
  2. 在 tty2 中运行命令后 `Ctrl+Alt+F1` 切回 tty1，确认 tty1 中原有内容保留
  3. 多次切换后确认各终端状态独立不丢失
  4. 验证 `chvt` 命令能通过程序切换终端
  5. 确认 `Ctrl+Alt+Fn` 在 X11/Wayland 环境下不冲突（如适用）

### 38. 支持 virtio 块设备多队列（Multi-Queue）

- **位置**: `kernel/comps/virtio/src/device/block/device.rs`（第 226 行 FIXME）
- **工作量**: 2-3 周
- **做法**: 实现 virtio-blk 的多队列机制，支持多处理器并发 I/O 请求
- **验证方法**:
  1. 在 SMP 配置下（`osdk run --qemu-args="-smp 4"`）运行磁盘 I/O benchmark（如 `fio`），对比单队列和多队列的吞吐量和 IOPS
  2. 多线程并发读写验证数据正确性
  3. 确认单核配置下仍然正常工作（回退到单队列）
  4. 运行 `osdk test` 确认无回归

### 39. 增加多 PCIe segment group 支持

- **位置**: `ostd/src/arch/x86/kernel/acpi/mod.rs`（第 133 行）、`kernel/comps/pci/src/arch/riscv/mod.rs`（第 55 行）
- **现状**: 只假定一个 segment group
- **工作量**: 2 周
- **做法**: 扩展 PCI 枚举逻辑以支持多个 segment group
- **验证方法**:
  1. 使用 QEMU 配置多个 PCIe domain（`-device pcie-root-port,bus=pcie.0,id=rp1` 等），启动后 `lspci` 应显示不同 domain 下的设备
  2. 验证不同 segment group 中的设备都能正常初始化和工作
  3. 验证配置空间访问（`lspci -vv`）使用正确的 segment 号
  4. 运行 `osdk test` 确认无回归

### 40. 完善 VFS 路径解析中的负 dentry 缓存

- **位置**: `kernel/src/fs/vfs/path/dentry.rs`（第 295、573 行）
- **现状**: 标记为 TODO，负 dentry 膨胀是性能问题
- **工作量**: 2-3 周
- **做法**: 设计并实现负 dentry 缓存策略，缓存查找失败的路径以避免重复查找
- **验证方法**:
  1. 性能测试：循环 `stat("/nonexistent/file")` 10000 次，对比启用负 dentry 缓存前后的耗时（应有显著改善）
  2. 验证缓存的时效性：创建被缓存为"不存在"的文件后，`stat` 应能找到（缓存应及时失效）
  3. 验证内存回收：大量负 dentry 不应导致不可回收的内存膨胀
  4. 运行 `osdk test` 确认无回归

### 41. 修复一批同子系统的 gvisor / LTP 测试

- **工作量**: 2-4 周
- **做法**: 挑选同一子系统（如文件操作、socket、信号、内存映射等）的 blocked 测试集中修复，系统性提升该子系统的兼容性
- **验证方法**:
  1. 将修复的测试用例从 blocklist 中移除
  2. 运行完整测试套件，确认移除的用例全部 PASS
  3. 统计该子系统 blocked 测试数量的减少（如从 30 个降到 5 个）
  4. 确认无回归：原有 PASS 的用例仍 PASS

### 42. 集成 packetdrill 网络测试套件

- **相关 Issue**: #2858
- **工作量**: 2 周
- **做法**: 集成 Google 的网络协议测试工具，用于验证 TCP/UDP 协议实现的正确性
- **验证方法**:
  1. 将 packetdrill 工具交叉编译为 Asterinas 可执行格式
  2. 运行 packetdrill 自带的 TCP 测试脚本（如 `tcp/simple_connect.script`）
  3. 验证 TCP 三次握手、数据传输、四次挥手的报文序列与脚本预期一致
  4. 逐步运行更多 packetdrill 脚本（边界条件、超时重传、窗口管理等）
  5. 将 packetdrill 集成到 CI 中自动运行

### 43. 完善 Netlink route 内核实现

- **位置**: `kernel/src/net/socket/netlink/route/kernel/mod.rs`
- **现状**: 应为 per-namespace socket（第 73 行 TODO），ACK 标志处理未完成（第 50 行）
- **工作量**: 2-3 周
- **做法**: 将 netlink route socket 改为 per-namespace，完善 ACK 处理
- **验证方法**:
  1. `ip addr add 10.0.0.1/24 dev eth0` 应成功配置网络地址（底层使用 netlink route）
  2. `ip link show` 应正确列出网络设备
  3. `ip route add default via 10.0.0.254` 应成功添加路由
  4. 发送 `NLM_F_ACK` 标志的 netlink 消息后应收到 ACK 回复
  5. 不同 network namespace 中的 netlink route socket 应独立

### 44. 增加更多 cgroup controller

- **位置**: `kernel/src/fs/fs_impls/cgroupfs/controller/mod.rs`
- **现状**: 仅实现 Memory、CPUSet、PIDs 三个控制器
- **工作量**: 每个 2-3 周
- **可新增**: CPU、Devices、Freezer、Blkio、Net_cls、PerfEvent 等常用控制器，参照已有实现模式
- **验证方法**（以 **Freezer controller** 为例）:
  1. `echo FROZEN > /sys/fs/cgroup/<group>/freezer.state` 后，该 cgroup 中所有进程应暂停（`kill -0 <pid>` 能检测到但进程不响应信号）
  2. `echo THAWED > /sys/fs/cgroup/<group>/freezer.state` 后，进程恢复运行
  3. 验证父子 cgroup 的冻结关系：冻结父 cgroup 时子 cgroup 也应冻结
  4. 运行 LTP 中 cgroup 相关测试
- **验证方法**（以 **CPU controller** 为例）:
  1. `echo 512 > /sys/fs/cgroup/<group>/cpu.shares` 设置权重，验证 CPU 时间按比例分配
  2. `echo 100000 > cpu.cfs_quota_us && echo 100000 > cpu.cfs_period_us` 限制为 100% 单核
  3. 运行 CPU 密集型任务验证限制生效

### 45. 实现 Go 标准库所需的全部系统调用

- **相关 Issue**: #1888（含优先级和复杂度评级）
- **工作量**: 每个 1-3 天
- **做法**: 按 Issue 中的优先级列表逐个实现缺失的系统调用
- **验证方法**:
  1. 实现每个系统调用后，编写最小用户态测试程序验证基本语义
  2. 逐步尝试编译并运行简单的 Go 程序（如 `fmt.Println("hello")`）
  3. 使用 Go 的 `syscall` 包测试各个已实现的系统调用
  4. 最终目标：Go 标准库中的网络、文件、并发等基础功能全部可用

---

## 四级 — 较难（1-3 个月）

涉及新子系统、跨模块改动或架构决策。

### 46. 增加 LSM 框架 + YAMA

- **优先级**: 高
- **位置**: `kernel/src/process/posix_thread/alien_access.rs`（第 57 行）
- **现状**: ptrace 访问检查处有明确 TODO 提到需要 YAMA
- **工作量**: 1-2 月
- **做法**: 先设计通用 LSM hook 机制（inode_permission、file_open、ptrace_access_check 等），然后实现 YAMA 模块（限制 ptrace 附加范围）
- **验证方法**:
  1. **LSM hook 注册**: 注册自定义 hook 后，对应操作（如 `open`）应调用 hook 函数
  2. **YAMA ptrace_scope=0**: 经典 ptrace 权限检查（与无 YAMA 时一致）
  3. **YAMA ptrace_scope=1**: 只能 ptrace 自身子进程，非子进程应返回 `EPERM`
  4. **YAMA ptrace_scope=2**: 只有 `CAP_SYS_PTRACE` 的进程才能 ptrace
  5. **YAMA ptrace_scope=3**: 完全禁止 ptrace（除非有 `CAP_SYS_PTRACE` 且通过 `prctl(PR_SET_PTRACER)` 明确授权）
  6. `cat /proc/sys/kernel/yama/ptrace_scope` 应显示当前级别
  7. 运行 `osdk test` 确认无回归

### 47. 增加 Network namespace

- **优先级**: 中（容器网络隔离必需）
- **工作量**: 2-3 月
- **做法**: 重构网络栈——网络接口、路由表、socket、iptables 等都需要 namespace 化，工作量较大
- **验证方法**:
  1. `unshare --net /bin/bash` 创建新 network namespace，确认 `ip link` 只看到 loopback
  2. 使用 veth pair 连接两个 namespace，验证跨 namespace 的 ping 通/不通
  3. 在不同 namespace 中分别启动 TCP server，验证端口隔离（相同端口在不同 namespace 可独立绑定）
  4. `ip netns add ns1 && ip netns exec ns1 ...` 完整工作流
  5. 运行 LTP 中 network namespace 测试

### 48. 增加 IPv6 支持

- **位置**: `kernel/src/net/socket/ip/addr.rs`（第 26 行 TODO）
- **工作量**: 2-3 月
- **做法**: 扩展 socket 地址结构、实现 IPv6 协议栈（NDP、ICMPv6、IPv6 路由）
- **验证方法**:
  1. 创建 `AF_INET6` socket，`bind("::1", 8080)` 后 `connect` 应成功
  2. `ping6 ::1` 环回地址应通
  3. `ping6 fe80::...%eth0` 链路本地地址应通（需 NDP）
  4. 配置全局 IPv6 地址后跨节点 `ping6` 应通
  5. `ip -6 addr show` 和 `ip -6 route show` 输出正确
  6. 运行 LTP 中 IPv6 相关测试

### 49. 多网卡和路由表

- **位置**: `kernel/src/net/iface/init.rs`（第 33 行）、`broadcast.rs`（第 11 行）
- **现状**: 硬编码单个网络设备，FIXME 提到应从路由表获取广播信息
- **工作量**: 1-2 月
- **做法**: 设计路由表抽象层，支持多网络接口和路由规则
- **验证方法**:
  1. 配置 QEMU 双网卡（`-netdev user,id=net0 -netdev tap,id=net1`），启动后 `ip link` 应显示两个网络接口
  2. `ip addr add 10.0.2.15/24 dev eth0 && ip addr add 192.168.1.1/24 dev eth1`
  3. `ip route add 10.0.0.0/8 via 10.0.2.1`，验证流量走 eth0
  4. `ip route add 192.168.0.0/16 via 192.168.1.254`，验证流量走 eth1
  5. 验证广播包从正确接口发出

### 50. 实现 mount propagation

- **位置**: `kernel/src/fs/vfs/path/mount.rs`（第 35 行）
- **现状**: 仅实现了 private 传播类型
- **工作量**: 1-2 月
- **做法**: 实现 shared/private/slave/unbindable 四种传播类型的完整语义
- **验证方法**:
  1. **shared**: `mount --make-shared /` 后，在 namespace A 中 `mount /dev/sda1 /mnt`，namespace B 中应看到 `/mnt` 被挂载
  2. **slave**: `mount --make-slave /` 后，主 namespace 的挂载事件传播到从 namespace，但从 namespace 的挂载不传播回去
  3. **private**: 默认行为，挂载事件不传播
  4. **unbindable**: `mount --make-unbindable /opt` 后，`mount --bind /opt /mnt2` 应失败
  5. 运行 LTP 中 mount propagation 测试

### 51. 增加 ext4 文件系统支持

- **优先级**: 高
- **现状**: 目前只有 ext2
- **工作量**: 2-3 月
- **做法**: 可基于现有 ext2 扩展，但需要实现 extent tree、h-tree 目录索引、日志（jbd2）等核心特性
- **验证方法**:
  1. 在宿主机上 `mkfs.ext4 /dev/sda` 创建 ext4 文件系统，在 Asterinas 中 `mount -t ext4 /dev/sda /mnt`
  2. 文件操作验证：创建/读取/写入/删除文件，目录操作，符号链接，硬链接
  3. 大文件测试：创建 > 4GB 文件验证 extent tree 正确性
  4. 断电恢复测试：写入过程中强制重启，重启后 `fsck.ext4` 应无错误（验证日志恢复）
  5. 运行 LTP ext4 测试套件
  6. 运行 `osdk test` 确认无回归

### 52. 增加 SCHED_DEADLINE 调度策略

- **工作量**: 1-2 月
- **做法**: 实现 EDF（Earliest Deadline First）调度算法和带宽管理（CBS），需要理解实时调度理论
- **验证方法**:
  1. 设置 `SCHED_DEADLINE` 参数（runtime=10ms, deadline=30ms, period=30ms），验证任务每 30ms 内至少获得 10ms CPU 时间
  2. 总带宽超过 100% 时 `sched_setattr()` 应返回 `EBUSY`
  3. `sched_yield()` 后任务应等到下一个 period 才能运行
  4. 运行 Linux 的 `sched_deadline` 测试用例
  5. 使用 `deadline_test`（schedutils 中的测试工具）验证

### 53. 增加 load tracking 调度

- **相关 Issue**: #1912
- **工作量**: 1-2 月
- **做法**: 实现类似 Linux CFS 的 PELT（Per-Entity Load Tracking）负载追踪机制
- **验证方法**:
  1. 运行 CPU 密集型任务，验证 `/proc/loadavg` 显示合理的负载值
  2. 停止所有任务后，负载应逐渐衰减到接近 0
  3. 在多个核上运行任务，验证每核负载和总负载都正确
  4. 验证负载均衡：多核间任务迁移决策基于负载追踪数据
  5. 对比 Linux `/proc/loadavg` 的行为

### 54. 支持内核空间大页映射

- **位置**: `ostd/src/mm/kspace/mod.rs`（第 261 行 TODO）
- **工作量**: 1-2 月
- **做法**: 在内核地址空间中支持 2MiB/1GiB 大页映射，减少 TLB miss
- **验证方法**:
  1. 启动日志中确认大页映射成功（如 "Using 2MiB pages for kernel .text"）
  2. 通过性能计数器对比 TLB miss 率：启用大页前后 `perf stat -e dTLB-load-misses` 应显著下降
  3. 运行内核 benchmark（如内存带宽测试）确认性能提升
  4. 运行 `osdk test` 确认无回归

### 55. 实现 TLB ASID 管理

- **相关 Issue**: #969
- **工作量**: 1-2 月
- **做法**: 为每个进程分配 ASID，减少上下文切换时的 TLB flush，需架构特定实现
- **验证方法**:
  1. 多进程场景下对比上下文切换前后的 TLB miss 数量：启用 ASID 后应显著减少
  2. `perf bench sched pipe` 测量上下文切换延迟，对比优化前后
  3. ASID 溢出处理：创建超过 ASID 位数（如 x86_64 上 12 位 = 4096 个）的进程，确认全局 TLB flush 后重新分配
  4. 运行 `osdk test` 确认无回归

### 56. 零原子操作文件表查找

- **相关 Issue**: #1550
- **工作量**: 1-2 月
- **做法**: 设计 per-CPU 或 RCU 方案消除文件描述符表查找中的原子操作开销
- **验证方法**:
  1. 微基准测试：`perf bench syscall pipe` 或自定义 `open/close/read/write` 循环，对比优化前后的吞吐量
  2. 多线程并发文件操作测试：多线程同时读写不同 fd，验证数据正确性
  3. `close()` 和 `dup2()` 等修改 fd 表的操作与新方案兼容
  4. 运行 `osdk test` 确认无回归

### 57. 减少 read/write 系统调用路径中的堆分配和内存拷贝

- **相关 Issue**: #1057
- **工作量**: 1-2 月
- **做法**: 分析热路径，使用零拷贝技术或栈上缓冲区减少堆分配
- **验证方法**:
  1. 使用 `perf record` 分析 `read`/`write` 系统调用路径，确认热路径中无 `kmalloc`/`kfree` 调用
  2. `dd if=/dev/zero of=/dev/null bs=4k count=100000` 测量吞吐量，对比优化前后
  3. 小 buffer（1 字节）和大 buffer（1MiB）场景分别测试
  4. 运行 `osdk test` 确认无回归

### 58. 调查并修复 SMP=8 下 sqlite 性能退化

- **相关 Issue**: #2485
- **工作量**: 1-2 月
- **做法**: 先通过 profiling 定位多核扩展性瓶颈（可能是锁竞争、缓存行 bouncing 等），再针对性优化
- **验证方法**:
  1. 在 SMP=1、2、4、8 下分别运行 sqlite benchmark（如 TPC-C 类负载），记录 QPS
  2. 绘制 scalability 曲线：理想情况下 QPS 应随核数线性增长
  3. 使用 `perf top` / `perf record` 定位热点函数和锁竞争点
  4. 优化后重新测试，确认 SMP=8 时性能有显著提升且不引入单核退化
  5. 运行 `osdk test` 确认无回归

### 59. 增加多用户支持

- **相关 Issue**: #1430
- **工作量**: 1-2 月
- **做法**: 实现 adduser、passwd、/etc/passwd、/etc/shadow，以及文件权限隔离和用户切换
- **验证方法**:
  1. `adduser testuser` 创建用户后 `/etc/passwd` 和 `/etc/shadow` 应包含新条目
  2. `su - testuser` 切换用户后 `whoami` 显示 `testuser`、`id` 显示正确的 UID/GID
  3. 文件权限隔离：用户 A 创建的文件（`chmod 600`），用户 B 无法读取
  4. `passwd testuser` 修改密码后，新密码可用于登录
  5. root 可以读写任意文件

### 60. 增加 device mapper + dm-crypt + dm-verity

- **优先级**: 高
- **工作量**: 2-3 月
- **做法**: 先实现 device mapper 块设备虚拟层，再在其上实现 dm-crypt（块级加密）和 dm-verity（完整性校验）
- **验证方法**:
  1. **device mapper 基础**: `dmsetup create test --table "0 1024 linear /dev/sda 0"` 创建映射设备，`dd` 读写验证数据正确
  2. **dm-crypt**: `cryptsetup luksFormat /dev/sda` 后挂载加密卷，写入数据、卸载、重新挂载后数据完整
  3. **dm-verity**: 创建 verity 镜像，挂载后读取正确；篡改底层块设备后读取应返回 `EIO`
  4. 运行 LTP 中 device mapper 相关测试
  5. 运行 `osdk test` 确认无回归

### 61. 增加 virtio-mmio 在 TDX 环境中的支持

- **位置**: `kernel/comps/virtio/src/transport/mmio/bus/mod.rs`（第 30 行 TODO）
- **工作量**: 1-2 月
- **做法**: 在 TDX 机密计算环境下安全地访问 virtio-mmio 设备
- **验证方法**:
  1. 在 TDX 环境中启动 Asterinas，确认 virtio-mmio 设备正常枚举
  2. 网络和块设备通过 virtio-mmio 正常工作（`ping`、`dd` 等）
  3. 验证 TDX 私有内存和共享内存的隔离：设备只能访问共享内存中的数据
  4. 对比非 TDX 环境下的行为一致性
  5. 运行 `osdk test` 确认无回归

### 62. 评估 PGO（Profile-Guided Optimization）对内核的效果

- **相关 Issue**: #760
- **工作量**: 1-2 月
- **做法**: 研究型任务，搭建 benchmark 流程，对比 PGO 前后的性能差异
- **验证方法**:
  1. 编译两个版本：普通编译和 PGO 编译（`-C profile-generate` → 运行 benchmark → `-C profile-use`）
  2. 运行一组代表性 benchmark：`sqlite`、`fio` 磁盘 I/O、`iperf` 网络带宽、`hackbench` 调度器
  3. 对比各 benchmark 的 PGO 前后数据，记录加速比
  4. 输出评估报告：PGO 对各子系统的影响程度、是否值得加入默认构建流程

### 63. 完善 RISC-V 架构支持

- **现状**: Tier 2，大量 TODO：FPU 上下文、IOMMU、硬件随机数生成器等
- **工作量**: 2-3 月
- **做法**: 逐个补全 `ostd/src/arch/riscv/` 中的缺失功能
- **验证方法**:
  1. 在 QEMU RISC-V 上 `osdk run` 启动内核，确认启动日志正常
  2. **FPU**: 运行浮点运算程序（`printf("%f\n", 3.14 * 2.0)`），验证结果正确
  3. **多核**: `osdk run --qemu-args="-smp 4"` 下所有核正常初始化，`/proc/cpuinfo` 显示 4 个核
  4. **定时器**: `sleep 1` 精度在合理范围内
  5. **IOMMU**: 设备 DMA 通过 IOMMU 映射正确
  6. 运行 `osdk test` 确认 RISC-V 下全部通过

### 64. 完善 LoongArch 架构支持

- **现状**: Tier 3，中断处理、SMP、FPU、定时器等都有 TODO；`ostd/src/arch/loongarch/irq/chip/mod.rs:11` 的 FIXME 提到需要支持 SMP
- **工作量**: 2-3 月
- **做法**: 逐个补全 `ostd/src/arch/loongarch/` 中的缺失功能
- **验证方法**:
  1. 在 QEMU LoongArch 上启动内核，确认启动日志正常
  2. **SMP**: 多核启动成功，核间通信和调度正常
  3. **中断**: 外部中断（定时器、UART）正常响应
  4. **FPU**: 浮点运算程序运行正确
  5. **定时器**: 定时精度合理
  6. 运行 `osdk test` 确认 LoongArch 下全部通过

### 65. IOMMU 支持完善

- **现状**: x86 上 IOMMU 启用有 bug（Issue #1517），RISC-V 和 LoongArch 的 IOMMU 未实现
- **工作量**: 1-2 月
- **做法**: 修复 x86 IOMMU bug，为 RISC-V 和 LoongArch 实现 IOMMU 驱动
- **验证方法**:
  1. **x86 bug 修复**: 重现 Issue #1517 中的 IOMMU 启用失败场景，确认修复后正常
  2. 设备直通（passthrough）测试：将网卡直通给 guest，验证 DMA 操作通过 IOMMU 映射正确
  3. 验证 IOMMU 隔离：设备不能访问分配给内核的私有内存
  4. **RISC-V**: 实现 IOMMU 驱动后进行上述相同测试
  5. 运行 `osdk test` 确认无回归

---

## 五级 — 最难（3 个月以上）

大规模架构工作或全新子系统。

### 66. 增加 io_uring 异步 I/O 子系统

- **优先级**: 中
- **所需系统调用**: `io_uring_setup`、`io_uring_enter`、`io_uring_register`
- **工作量**: 3-6 月
- **做法**: 实现 submission queue / completion queue、poll 机制、文件注册、内核侧异步 I/O 路径
- **挑战**: 现代 Linux 高性能 I/O 的核心，设计复杂度极高
- **验证方法**:
  1. **基础**: `io_uring_setup(32, &params)` 返回有效的 ring fd，`mmap` 共享环形缓冲区
  2. **读写**: 提交 `IORING_OP_READ` 和 `IORING_OP_WRITE` SQE，验证 CQE 返回正确结果
  3. **poll**: 提交 `IORING_OP_POLL_ADD`，事件触发后收到 CQE
  4. **多并发**: 提交 1000 个并发读请求，验证全部完成且数据正确
  5. **liburing 测试**: 使用 `liburing` 库运行其自带测试套件
  6. **性能**: 与同步 `read`/`write` 对比，在高并发场景下应显著更快
  7. 运行 `osdk test` 确认无回归

### 67. 增加 ARM（AArch64）体系架构支持

- **优先级**: 高
- **工作量**: 6+ 月
- **做法**: 需要在 ostd 中实现完整的 AArch64 架构层（CPU 上下文、页表、中断、SMP 等）、kernel 中适配架构特定代码、osdk 中添加架构支持
- **挑战**: 全新架构移植，涉及面极广
- **验证方法**:
  1. **最小启动**: 在 QEMU AArch64（`qemu-system-aarch64 -machine virt`）上启动到 shell
  2. **基本操作**: 在 shell 中执行 `ls`、`cat /proc/cpuinfo`、`echo hello`
  3. **多核**: `-smp 4` 下所有核正常初始化
  4. **设备**: virtio-net、virtio-blk、UART 正常工作
  5. **用户态**: 编译并运行简单 C 程序（aarch64 交叉编译）
  6. 运行 `osdk test` 在 AArch64 目标上全部通过

### 68. 实现可扩展的引用计数（scalable refcount）

- **相关 Issue**: #1529
- **工作量**: 3-4 月
- **做法**: 实现类似 RadixVM RefCache 的机制，解决多核下页面引用计数的可扩展性问题
- **挑战**: 核心 MM 基础设施改动，需要仔细验证正确性
- **验证方法**:
  1. **正确性**: 运行所有现有 `osdk test`，确认无回归
  2. **并发正确性**: 多线程并发 `fork`/`exec`/`exit` 压力测试（如 `fork bomb` 受限版本），确认无 panic 或数据损坏
  3. **可扩展性**: SMP=1/2/4/8 下运行页面密集型 benchmark（如 `mmap` 大量页面后 `fork`），对比优化前后的吞吐量曲线
  4. **内存开销**: 确认 RefCache 的内存开销在可接受范围内

### 69. 实现队列自旋锁（queued spinlock）

- **相关 Issue**: #1528
- **工作量**: 3-4 月
- **做法**: 实现类似 Linux qspinlock 的机制，改善高竞争场景下的锁性能
- **挑战**: 核心同步原语改动，所有依赖 spinlock 的代码都受影响
- **验证方法**:
  1. **正确性**: 运行所有 `osdk test`，重点运行多线程和 SMP 相关测试
  2. **无死锁**: 运行长时间 SMP 压力测试（> 1 小时），确认无死锁
  3. **公平性**: 多核争同一把锁时，验证无饿死（每核最终都能获得锁）
  4. **性能**: 高竞争场景 benchmark（如多核同时原子更新共享数据结构），对比 ticket lock 和 qspinlock 的吞吐量
  5. **低竞争场景**: 确认 qspinlock 在低竞争时性能不退化

### 70. 增加 NUMA 支持

- **位置**: `kernel/src/syscall/getcpu.rs`（第 13 行 TODO）
- **工作量**: 3-6 月
- **做法**: 实现 NUMA 拓扑发现、节点感知的内存分配和调度策略
- **挑战**: 架构级改动，涉及调度器、内存分配器、设备亲和性等多个子系统
- **验证方法**:
  1. 使用 QEMU NUMA 模拟（`-smp 4 -numa node,cpus=0-1,nodeid=0 -numa node,cpus=2-3,nodeid=1`）
  2. `numactl --hardware` 显示正确的 NUMA 拓扑
  3. 内存分配策略验证：`numactl --membind=0` 分配的内存在 node 0 上
  4. 调度亲和性验证：进程优先在分配了其内存的 node 上运行
  5. 性能测试：对比 NUMA 感知和 NUMA 无感知的内存访问延迟

### 71. 重构 page cache 系统

- **相关 Issue**: #2937
- **工作量**: 3-6 月
- **做法**: 重新设计页面缓存架构，改进缓存策略和一致性管理
- **挑战**: 核心 MM 子系统重设计，影响文件系统和内存管理的交互
- **验证方法**:
  1. **正确性**: 所有文件系统测试通过（ext2 读写、procfs、sysfs 等）
  2. **一致性**: `write` 后立即 `read` 应看到最新数据；`mmap` 写入后 `read` 也应看到
  3. **回写**: dirty page 应在合适的时机回写到磁盘（`sync` 命令、定时回写、内存压力）
  4. **性能**: 文件 I/O benchmark 对比重构前后（`dd`、`fio` 随机读写）
  5. **内存回收**: 在内存压力下 page cache 应能被正确回收
  6. 运行 `osdk test` 确认无回归

---

## 推荐路线

### 个人/实习生快速上手

```
一级 #1 (getrandom flags) → 一级 #3 (rename flags) → 一级 #4 (MAP_32BIT)
    → 二级 #9 (mlock) 或 二级 #10 (rlimit 子任务) → 二级 #12 (细粒度日志)
```

### 做一个有分量但可控的独立项目

- **#23 seccomp** — 容器安全核心，scope 清晰，有 Linux 参考
- **#25 IPC namespace** — 有模式可循（参照已有 namespace 实现）
- **#27 procfs 完善** — 可拆分为多个小 PR，每个 inode 独立

### 容器场景团队长期规划

```
#46 (LSM + YAMA) → #23 (seccomp) → #47 (Network namespace)
    → #50 (mount propagation) → #60 (device mapper)
```

### 性能优化方向

```
#17 (测试速度) → #57 (read/write 路径) → #56 (文件表查找)
    → #55 (TLB ASID) → #54 (大页映射)
```

### 新架构移植方向

```
#63 (RISC-V 完善) 或 #64 (LoongArch 完善) → #67 (AArch64 新增)
```
