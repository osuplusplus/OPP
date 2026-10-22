//! Malody `.mcz` 谱面批量转换为 osu! `.osz`：委托 `mania_converter` 库
//!（内部按 ZIP 解包、重组资源后打包），逐文件独立处理，单个失败不影响
//! 其余文件；输出为源文件同目录同名 `.osz`，已存在时不覆盖而是跳过。

use std::path::{Path, PathBuf};

use crate::error::{CommandError, CommandResult};

use super::models::{ManiaConversionItem, ManiaConversionResult};

/// 转换单个文件：校验存在且扩展名为 `.mcz`，目标 `.osz` 已存在则跳过，
/// 否则交给 `process_mcz_file` 处理（第二参数传 `false` 跳过库内星级
/// 计算）；所有失败都包装成状态条目，不向上抛错。
fn convert_one(input: String) -> ManiaConversionItem {
    let path = PathBuf::from(&input);
    let invalid = |message: &str| ManiaConversionItem {
        input: input.clone(),
        status: "failed".into(),
        output: None,
        message: Some(message.into()),
    };
    if !path.is_file() {
        return invalid("文件不存在或不可读取");
    }
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mcz"))
    {
        return invalid("仅支持 .mcz 文件");
    }
    let output = path.with_extension("osz");
    if output.exists() {
        return ManiaConversionItem {
            input,
            status: "skipped".into(),
            output: Some(output.display().to_string()),
            message: Some("目标 .osz 已存在，未覆盖".into()),
        };
    }
    match mania_converter::malody_func::process_mcz_file(Path::new(&path), false) {
        Ok((output, _)) => ManiaConversionItem {
            input,
            status: "completed".into(),
            output: Some(output.display().to_string()),
            message: None,
        },
        Err(error) => ManiaConversionItem {
            input,
            status: "failed".into(),
            output: None,
            message: Some(error.to_string()),
        },
    }
}

/// 批量转换 `.mcz` 到 `.osz`：转换在后台线程逐个执行，避免大量解包/打包
/// 阻塞命令线程；逐文件结果汇总为 `ManiaConversionResult` 返回。
#[tauri::command]
/// 供前端调用的 Tauri 命令：完成该功能模块的业务操作。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn convert_mania_beatmaps(paths: Vec<String>) -> CommandResult<ManiaConversionResult> {
    if paths.is_empty() {
        return Err(CommandError::new(
            "NO_CONVERSION_INPUT",
            "请先选择至少一个 .mcz 文件",
        ));
    }
    let items = crate::infrastructure::tasks::background("tools", move || {
        paths.into_iter().map(convert_one).collect()
    })
    .await
    .map_err(|error| CommandError::new("CONVERSION_TASK_FAILED", error.to_string()))?;
    Ok(ManiaConversionResult { items })
}

#[cfg(test)]
mod tests {
    use super::convert_one;

    #[test]
    fn rejects_non_mcz_files() {
        let result = convert_one("C:/missing/chart.osu".into());
        assert_eq!(result.status, "failed");
        assert!(result.message.is_some());
    }
}
