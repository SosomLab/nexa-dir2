//! 내장 위젯 모음 — 리스트(M1)·경로 바(M2-1)·탭 바(M2-2)·메뉴/도구/상태바(M2-3)·하단 도크(M4-1).

pub mod chrome;
pub mod dock;
pub mod menubar;
pub mod pathbar;
pub mod rows;
pub mod tabbar;

pub use chrome::{StatusBar, ToolButton, Toolbar};
pub use dock::InfoDock;
pub use menubar::{Menu, MenuBar, MenuItem};
pub use pathbar::{split_path, PathBar, Segment};
pub use rows::{Marker, RowItem, RowSource, ScrollAlign, SelectOp, ViewMode, VirtualRows};
pub use tabbar::{TabAction, TabBar};
