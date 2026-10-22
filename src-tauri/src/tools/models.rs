use serde::Serialize;

/// mania 谱面批量转换中单个文件的处理结果，供前端逐条展示。
#[derive(Debug, Clone, Serialize)]
pub struct ManiaConversionItem {
    /// 用户选择的输入文件原始路径。
    pub input: String,
    /// 处理状态：`completed`、`skipped`（目标 `.osz` 已存在）或 `failed`。
    pub status: String,
    /// 成功或跳过时输出 `.osz` 的路径；失败时为 `None`。
    pub output: Option<String>,
    /// 补充说明（失败或跳过原因）；成功时为 `None`。
    pub message: Option<String>,
}

/// 一次批量转换的汇总结果，逐文件明细见 `items`。
#[derive(Debug, Clone, Serialize)]
pub struct ManiaConversionResult {
    pub items: Vec<ManiaConversionItem>,
}
