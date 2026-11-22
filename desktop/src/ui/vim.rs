//! Vim 模式：Esc 进入 normal（光标居中），v 显示行首字母标签跳转，
//! V 开始可视选择，Ctrl+A 高亮/加笔记。
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum VimMode {
    #[default]
    Off,
    Normal,
    Visual,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct VimState {
    pub mode: VimMode,
    pub labels: bool,
    pub cursor: (usize, usize), // (block, char)
    pub start: (usize, usize),
    pub placed: bool, // 光标是否已落到可见块上（开启后首次渲染时定位）
}

impl VimState {
    pub fn toggle(&mut self) {
        self.mode = if self.mode == VimMode::Off {
            VimMode::Normal
        } else {
            VimMode::Off
        };
        self.labels = false;
        self.cursor = (0, 0);
        self.start = (0, 0);
        self.placed = false;
    }
    pub fn active(&self) -> bool {
        self.mode != VimMode::Off
    }
}