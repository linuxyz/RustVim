# RVim：Rust 实现的 Vim

**状态：** 16 个 crate 组成的工作区提供 TUI 与 headless 编辑器；P0 与 P1
已全部完成——177 个 P0/P1 目录特性均已实现并有测试覆盖（编辑、正则搜索、
VimL、syntax、runtime/help、窗口/tab、映射、autocmd、quickfix/tags/sessions
等日常可用面），并且已实现 P2 的折叠、signs、text properties 与两路 diff
子系统。逐项状态、已记录的近似项与剩余 P2/P3 工作见
[docs/STATUS.md](docs/STATUS.md)。

**目标：** 对齐 Vim 9.2.1338 的可观察行为。  
**范围：** Vi/Ex 兼容模式不做；GUI（只留接口）、图像协议暂缓；
终端模拟器（<code>:terminal</code>、Terminal-Job、<code>term_*</code>）已移除。

---

## 交付内容

| 文档 | 内容 |
|---|---|
| [docs/FEATURES.md](docs/FEATURES.md) | **346 个可选功能**，含稳定 ID、阶段、复杂度、依赖和范围。先看这里。 |
| [docs/STATUS.md](docs/STATUS.md) | 已实现的 P0/P1 范围、测试结果和已知缺口。 |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | 分层、crate 图、状态所有权、输入/渲染流水线、风险与决策。 |
| [docs/UI-PROTOCOL.md](docs/UI-PROTOCOL.md) | TUI/GUI/headless 的统一接口：Frame、Cell、Attr、事件、能力协商。 |
| [docs/WINDOWS.md](docs/WINDOWS.md) | 为什么 Window 属于表示层；布局、viewport、tab、鼠标。 |
| [docs/SCRIPTING.md](docs/SCRIPTING.md) | 旧版 Vim script 的值模型、GC、解析器、求值器和 host 边界。 |
| [docs/QUICKJS.md](docs/QUICKJS.md) | QuickJS 嵌入、<code>vim</code> API、类型转换、模块、Promise、沙箱。 |
| [docs/RUNTIME-COMPAT.md](docs/RUNTIME-COMPAT.md) | 运行时加载顺序、filetype/ftplugin/indent/syntax/help、Tier A/B/C 和测试。 |
| [docs/ROADMAP.md](docs/ROADMAP.md) | M0–M6 里程碑、验收门槛、推荐的第一条纵向切片。 |
| [docs/EXCLUSIONS.md](docs/EXCLUSIONS.md) | Vi/Ex、Vim9、GUI、终端模拟器、可选功能的行为契约。 |
| [docs/VIM9-PLAN.md](docs/VIM9-PLAN.md) | Vim9 脚本实现计划、阶段门与进度（runtime 级已启用）。 |
| [README.md](README.md) | 英文入口。 |

机械清单（由 Vim runtime 自动生成）在 [docs/inventory/](docs/inventory/README.md)：
593 个 Ex 命令、695 个模式按键、456 个选项规格、605 个内建函数、136 个
autocommand 事件、211 个 <code>has()</code> 标志。清单来自 tag
<code>v9.2.1338</code>，已排除 <code>testdir/</code> 测试夹具。

---

## 核心设计（对应你的约束）

### 1. 三层架构，依赖单向

~~~text
后端（rvim-tui / 未来 rvim-gui / rvim-headless）
    ↑ Frame（cell 网格）   ↓ UiEvent
表示层（rvim-view）
    — Window、TabPage、布局、viewport、fold 显示
    — statusline、tabline、cmdline、消息、补全菜单
    — compositor 生成 Frame
    ↑ 只读模型 + 命令   ↓ 修改请求
编辑器核心（buffer、undo、options、寄存器、Ex、VimL、regex、syntax、runtime、jobs、QuickJS）
~~~

- **tabline / statusline / cmdline 不绑定 UI**：它们在 <code>rvim-view</code>
  中求值 Vim 选项/表达式并输出 cell segment，不引用任何终端或 GUI crate。
  这是编译期依赖规则，不是口头约定。
- **Window 与 UI 层结合**：Window 属于表示层，拥有 cursor、scroll、fold、
  window-local options；buffer 模型不知道 Window 存在。TUI/GUI 只消费最终
  Frame。
- **TUI/GUI 接口**：<code>UiBackend</code> 负责 present/poll/resize/clipboard/
  suspend，核心只发 cell 和收语义事件；鼠标点击由核心的 hit region 解析，
  后端不知道 tab/window/statusline 的含义。
- **GUI 暂不实现**：保留 <code>UiCapabilities</code> 和 GUI 扩展 trait；
  <code>has('gui')</code> 返回 0。

### 2. Vim runtime 是兼容性判据

固定 Vim 9.2.1338 的 <code>runtime/</code>，不做本地修改。兼容性按文件度量：
plugin、ftplugin、indent、syntax、colors、compiler、help tags。Tier A 必须
100% 通过；Vim9/terminal/GUI 的缺口要分类记录，不能静默失败。详见
[RUNTIME-COMPAT.md](docs/RUNTIME-COMPAT.md)。

### 3. 两个脚本引擎，一个边界

- **旧版 VimL** 是兼容性引擎：字节字符串、mark-sweep GC（允许循环引用）、
  AST 解释器、autoload、异常、<code>:execute</code>/<code>:redir</code>。
- **QuickJS** 是现代扩展引擎：<code>vim.cmd/eval/fn/api</code>、JS 函数与
  Funcref 互转、ES module、Promise 与事件循环、内存/栈/超时限制、沙箱。
- QuickJS 不替代 VimL，也不改变 Vim 的 <code>has()</code> 语义。

### 4. 范围决策

| 项目 | 决策 |
|---|---|
| Vim 的 Ex 命令语言 | 实现（这是 Vim 的命令行，不是 Vi 兼容模式） |
| Vi/Ex 兼容模式、<code>-e</code>/<code>-E</code>、<code>'compatible'</code> | 不做 |
| Vim9script | runtime 级支持，<code>has('vim9script')</code> 为 1；近似项见 EXCLUSIONS 第 3 节 |
| GUI | 只定义接口，不实现 |
| 终端模拟器 | **已移除**；<code>has('terminal')</code> 为 0；jobs/channels 保留 |
| QuickJS | 实现，作为可选 Cargo feature |
| 运行时兼容 | 实现，分 Tier A/B/C |

终端移除的细节和错误契约见 [docs/EXCLUSIONS.md](docs/EXCLUSIONS.md) 第 5 节。

---

## 如何选择功能

功能清单按领域分组，每行格式：

<pre>
| ID | 功能 | 阶段 | 复杂度 | 依赖 | 范围 |
</pre>

阶段：P0 骨架、P1 日常可用、P2 Vim 对齐、P3 生态/运行时、P4 可选或暂缓。  
复杂度：S/M/L/XL，是相对工作量，不是天数。

### 推荐批次

~~~sh
# 29 个 P0：最小可运行编辑器
python3 tools/select_features.py --bundle walking --with-deps

# 177 个 P0+P1：可日常替代 Vim
python3 tools/select_features.py --bundle daily --with-deps

# 脚本栈：VimL、builtins、regex、QuickJS、jobs
python3 tools/select_features.py --bundle scripting --with-deps

# 运行时兼容：RT/TAG/SYN/QF/SPELL
python3 tools/select_features.py --bundle runtime --with-deps

# 指定 ID
python3 tools/select_features.py --ids EDT-10,REGEX-02,JS-08 --with-deps
~~~

工具会补全传递依赖、按拓扑排序输出，并拒绝未知 ID。机器可读清单位于
<code>catalog/features.json</code>。

### 如果你只想先做一个最小版本

推荐先做 <code>walking</code>（M0+M1）：打开文件、移动、插入、删除、undo、
保存、退出；TUI 和 headless 渲染同一帧；状态栏/cmdline 已有独立 widget；
逐键与 Vim 做差分测试。这条纵切会先把 UI 契约、字节模型、undo 和 Ex 分发
压到真实。

---

## 说明：9.2.1338 清单

机械清单已从 tag <code>v9.2.1338</code> 重新生成，archive SHA-256 为
<code>eeb4370a8db32041af1c3b05159bdeeea6b781f5a32d0f1f68bd0c51d45880f3</code>；
<code>testdir/</code> 测试夹具不计入统计。tag 或校验和变化时重新运行：

~~~sh
python3 tools/gen_inventory.py --runtime /path/to/vim/runtime \
  --label "Vim 9.2.1338" --out docs/inventory
~~~

新增/变化的命令、选项、函数、事件和 <code>has()</code> 标志已在
[功能清单](docs/FEATURES.md) 的“Vim 9.2.1338 delta”一节逐项归类；
重新生成后若出现新条目，仍按“实现/暂缓/排除”处理。

---

## 下一步

告诉我你要实现的 ID、领域或批次。我会据此：
1. 生成依赖闭合的实现 backlog；
2. 创建对应的 Cargo workspace 和 crate 骨架；
3. 先做 UI 契约 + headless 测试，再按里程碑推进。

如果你需要，我也可以先把 <code>walking</code> 批次整理成可执行的迭代计划。
