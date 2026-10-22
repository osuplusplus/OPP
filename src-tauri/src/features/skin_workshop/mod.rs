//! 皮肤资源浏览、组件预览、配置合并与安全写入。

// Tauri 命令层：仅做参数反序列化与任务调度，不含业务逻辑。
mod commands;
// skin.ini 解析与保持原编码、换行风格的结构化回写。
mod config;
// 与前端共享的序列化契约：动作、写入模式与结果。
mod models;
// 业务编排：OSK 导入、组件替换、预设迁移与原子发布。
mod service;
// 资产扫描与按“连字符分组 / @2x / 帧号”规则构建目录树。
mod tree;

// 对外仅暴露命令（供 commands.rs 注册表引用）与服务类型（供 state.rs 组装）。
pub use commands::*;
pub use service::SkinWorkshopService;
