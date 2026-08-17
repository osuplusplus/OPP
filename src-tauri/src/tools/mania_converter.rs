use std::path::{Path, PathBuf};

use tauri::async_runtime;

use crate::error::{CommandError, CommandResult};

use super::models::{ManiaConversionItem, ManiaConversionResult};

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
    let items = async_runtime::spawn_blocking(move || paths.into_iter().map(convert_one).collect())
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
