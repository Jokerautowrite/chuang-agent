//! 创 · 产品视觉主色（雷蛇绿）
//!
//! **已定稿：主基调 = Razer Green。** 这是产品特色，不是临时装饰。
//! 终端 TUI、后续 GUI/文档配色一律以本文件 token 为准。
//!
//! 色值来源：Razer 品牌绿常用值 RGB(68, 214, 44) / 近似 #44D62C。
//! 老爸若用 Figma 出终稿色板，只改这里的常量，不要在各处硬编码散落 RGB。

use ratatui::style::Color;

/// 终端底板 · 纯黑（整屏底，不用主题灰）
pub const BG: Color = Color::Rgb(0, 0, 0);

/// 主色 · 雷蛇绿（边框、提示符、thinking、强调）
pub const BRAND: Color = Color::Rgb(68, 214, 44);

/// 主色提亮（系统提示、可读强调）
pub const BRAND_SOFT: Color = Color::Rgb(150, 235, 130);

/// 主色压暗（工具过程、次要信息）
pub const BRAND_DIM: Color = Color::Rgb(48, 120, 48);

/// 更暗的绿灰（脚注、chip 次要段）
pub const BRAND_MUTED: Color = Color::Rgb(90, 130, 90);

/// 用户气泡底（淡绿 wash，黑底上隐约发光）
pub const USER_BG: Color = Color::Rgb(14, 36, 16);

/// 用户气泡字
pub const USER_FG: Color = Color::Rgb(198, 255, 188);

/// 助手正文（微冷白，略带绿调，避免刺眼纯白）
pub const ASSIST_FG: Color = Color::Rgb(226, 232, 226);

/// 危险/失败（仅错误，不参与主色）
pub const DANGER: Color = Color::Rgb(220, 88, 88);

/// 输入框内已键入文字
pub const INPUT_FG: Color = Color::Rgb(240, 255, 240);

/// 占位符
pub const PLACEHOLDER: Color = Color::Rgb(70, 100, 70);
