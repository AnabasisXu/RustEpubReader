# EPUB Reader（个人 fork）

[![CI](https://github.com/AnabasisXu/RustEpubReader/actions/workflows/ci.yml/badge.svg)](https://github.com/AnabasisXu/RustEpubReader/actions/workflows/ci.yml)

基于 [RustEpubReader](https://github.com/zhongbai2333/RustEpubReader)（Apache-2.0）的本地化分支，作者 AnabasisXu。
目标：只读 EPUB、轻量、原生排版（无 WebView），主要强化高亮、笔记与标签工作流。

本文件记录**本 fork 的全部改动**、数据结构、快捷键与测试方式。

---

## 1. 功能改动

- `F11` 切换全屏；全屏状态写入设置，下次启动自动恢复。
- 双击 EPUB 启动时优先打开该文件，不再先恢复上次书籍；修复启动时标题字体未注册导致的闪退。
- `Ctrl+F` 打开搜索并直接聚焦输入框；回车执行；`↑`/`↓` 选择结果并自动滚动，回车跳转。

### 1.1 阅读
- 鼠标**不再翻页**（默认关闭）：单击左右侧、页面边缘拖拽都不再翻页，鼠标只用于选字；设置里可重新开启（`mouse_page_turn`）。
- 鼠标悬停在某条高亮的字符区间内 → 只显示该条高亮的笔记；同段多条高亮互不覆盖。
- 高亮文字使用高饱和前景色：黄 `#FFC400`、绿 `#00BE5A`、蓝 `#3C96FF`、粉 `#FF5096`。

### 1.2 高亮 / 笔记
- `Ctrl+A` 或 `H`：有选区 → 高亮选区（逐块写入）；无选区 → 高亮当前页第一个正文段落。
- 高亮后弹出「笔记」窗口：多行笔记 + 标签增删 + 保存/删除/取消；`Esc` 取消，`Ctrl+Enter` 保存。
- 点击已有高亮打开同一「笔记」窗口：可改笔记、增删标签、用「删除高亮」删除。
- 选区工具条移除「翻译 / 查词典」。

### 1.3 笔记页（左侧导航「笔记」，快捷键 F9）
- 跨书按书分组显示全部笔记：高亮原文（琥珀色）+ 笔记（淡蓝）+ 章节。
- 搜索框打开即聚焦、无提示语；匹配高亮原文 / 笔记正文 / 书名 / 标签。
- 行操作：点击跳转、编辑、标签、删除；复选框 + 全选/清空；`Ctrl+A` 全选 / 再按取消全选。
- 批量删除：`Delete` 或「删除选中」→ 确认框默认聚焦「删除」，`Enter` 确认，`Esc` 取消。
- `Esc` 分层：先取消确认框/标签框，再按一次返回阅读。
- `Alt+E` 导出（有勾选导出勾选，否则导出当前筛选）为 Markdown。

### 1.4 标签
- 全局标签集合 `tags.json`；每条高亮 `tags: string[]`，可多标签。
- `Ctrl+B` 面板第三页签为「标签」，点击按标签筛选；笔记页顶部标签按钮同样作为过滤词。
- `Alt+T`：笔记弹窗聚焦标签输入；阅读页打开标签页签。

### 1.5 书签与快速打开
- `Ctrl+M`：添加/取消当前块书签，并显示「已添加书签 / 已取消书签」淡出提示。
- `Ctrl+P`：VSCode 风格书籍选择器，输入过滤、`↑/↓` 选择、`Enter` 打开、`Esc` 关闭。

### 1.6 关于
- 作者/仓库改为 AnabasisXu；保留 Apache-2.0 与上游版权声明。

### 1.7 相对原版的精简与强化
相对原版做了大幅精简：移除多平台同步、阅读统计、TTS、翻译/词典、CSC、字体库导入、书籍分享/Portal、GitHub 登录、备份包等，桌面端只保留单机阅读。

主要强化高亮、笔记与标签：统一备注弹窗、按高亮区间显示笔记、笔记页按书聚合/筛选/导出、标签增删与筛选、全文搜索与独立快捷键设置。

---

## 2. 默认快捷键

| 动作 | 默认键 | 说明 |
| --- | --- | --- |
| 高亮当前选区/段落 | `Ctrl+A` 或 `H` | 有选区优先 |
| 撤销上一条高亮 | `Ctrl+Z` | 弹窗聚焦时留给文本编辑 |
| 笔记弹窗 取消 / 保存 | `Esc` / `Ctrl+Enter` | 全局处理 |
| 删除选中高亮 | `Delete` | 阅读页或笔记页 |
| 书签开关 | `Ctrl+M` | 带提示 |
| 快速打开书籍 | `Ctrl+P` | 上下键 + Enter |
| 打开笔记页 | `F9` | 也可点右上角「笔记」按钮 |
| 书签与笔记面板 | `Ctrl+B` | 当前书 |
| 标签页签 / 聚焦标签 | `Alt+T` | 阅读页 / 弹窗 |
| 导出 Markdown | `Alt+E` | 当前书或笔记页筛选结果 |
| 打开文件 / 返回书库 | `Ctrl+O` / `Ctrl+W` | |
| 搜索 / 目录 / 设置 | `Ctrl+F` / `Ctrl+T` / `Ctrl+,` | |
| 快捷键设置 | `Ctrl+K` 然后 `Ctrl+S` | 顺序可颠倒 |
| 全屏切换 | `F11` | 状态记忆仍有 bug，见下 |
| 笔记搜索（跨书） | `Ctrl+Shift+F` | |

所有键位可在 设置 →「快捷键与鼠标」中「录制」修改，空值回落默认。

---

## 3. 数据结构

`<数据目录>/books/<uuid>.json`（BookConfig）：

```json
{
  "highlights": [
    {
      "id": "uuid",
      "chapter": 5,
      "start_block": 12,
      "start_offset": 0,
      "end_block": 12,
      "end_offset": 48,
      "color": "Yellow",
      "created_at": 1780000000,
      "tags": ["AGI", "重要"],
      "text": "高亮原文"
    }
  ],
  "notes": [{"highlight_id": "uuid", "content": "笔记正文", "created_at": 0, "updated_at": 0}],
  "bookmarks": [{"chapter": 5, "block": 12, "created_at": 0}]
}
```

- `tags`、`text` 为新增字段，旧数据缺省为空（`text` 为空时回退到 EPUB 解析缓存）。
- 全局标签：`<数据目录>/tags.json`（字符串数组）。
- 数据目录 = 启动时的当前工作目录（通常为 exe 所在目录）。

---

## 4. 构建

Linux（gb 上已用）：

```bash
cargo build --release -p rust_epub_reader
```

Windows（Linux 交叉编译）：

```bash
cargo build --release --target x86_64-pc-windows-gnu -p rust_epub_reader
# 产物 target/x86_64-pc-windows-gnu/release/rust_epub_reader.exe
```

## 5. 测试

- 单元 / 集成：`cargo test -p reader_core --release`、`cargo test -p rust_epub_reader --release`
- Linux UI：`/data/workspace/test-loop.sh`（X11 + xdotool + OCR + JSON 断言）
- Windows 真机脚本（本机）：`win-e2e.ps1`（拖选→Ctrl+A→截图+像素/JSON）、`win-notes.ps1`（F9 笔记页）、`win-quick.ps1`（Ctrl+M/Ctrl+P）

## 6. 已知问题

- `F11` 全屏后，下次启动不会恢复全屏（Windows 实测；已记录，待修）。
- 尚未记忆最大化窗口；期望行为是记住最大化状态与窗口尺寸，下次启动恢复。
- 悬停笔记按高亮字符区间命中，不做到逐字形。
