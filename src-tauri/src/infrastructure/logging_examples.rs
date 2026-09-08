// 重构示例：为现有代码添加完善的日志
//
// 这个文件展示了如何将现有代码改造为使用新的日志系统

use crate::error::{CommandError, CommandResult};
use crate::infrastructure::logging::{global, finish_span};
use std::fs;
use std::path::{Path, PathBuf};

// ============================================================
// 示例 1: 简单的文件操作
// ============================================================

// 重构前：没有日志
pub fn read_config_old(path: &Path) -> CommandResult<String> {
    let content = fs::read_to_string(path)?;
    Ok(content)
}

// 重构后：添加完整日志
pub fn read_config_new(path: &Path) -> CommandResult<String> {
    let mut span = global().map(|logger| logger.operation("config", "read_config"));

    // 记录文件读取操作
    let result = fs::read_to_string(path);
    if let Some(ref s) = span {
        s.fs_op("read", path, &result);
    }

    match result {
        Ok(content) => {
            if let Some(ref mut s) = span {
                s.finish_ok(Some(serde_json::json!({
                    "path": path.to_string_lossy(),
                    "size": content.len(),
                })));
            }
            Ok(content)
        }
        Err(e) => {
            let error = CommandError::from_error("CONFIG_READ_ERROR", e);
            if let Some(ref mut s) = span {
                s.finish_error(&error);
            }
            Err(error)
        }
    }
}

// ============================================================
// 示例 2: 复杂的多步骤操作
// ============================================================

// 重构前：没有日志
pub async fn download_and_extract_old(url: &str, dest: &Path) -> CommandResult<PathBuf> {
    let response = reqwest::get(url).await?;
    let bytes = response.bytes().await?;
    let temp_path = dest.join("temp.zip");
    fs::write(&temp_path, bytes)?;

    let extract_path = dest.join("extracted");
    fs::create_dir_all(&extract_path)?;

    // 假设有一个 extract 函数
    extract_zip(&temp_path, &extract_path)?;
    fs::remove_file(&temp_path)?;

    Ok(extract_path)
}

// 重构后：添加每一步的日志
pub async fn download_and_extract_new(url: &str, dest: &Path) -> CommandResult<PathBuf> {
    let mut span = global().map(|logger| logger.operation("downloader", "download_and_extract"));

    // 步骤 1: 下载文件
    if let Some(ref s) = span {
        s.info(
            format!("开始下载文件: {}", url),
            Some(serde_json::json!({ "url": url }))
        );
    }

    let response = reqwest::get(url).await.map_err(|e| {
        let error = CommandError::network(format!("下载失败: {}", e));
        if let Some(ref mut s) = span {
            s.finish_error(&error);
        }
        error
    })?;

    let status = response.status().as_u16();
    if let Some(ref s) = span {
        s.http_request("GET", url, Some(status));
    }

    if !response.status().is_success() {
        let error = CommandError::network(format!("HTTP 错误: {}", status));
        if let Some(ref mut s) = span {
            s.finish_error(&error);
        }
        return Err(error);
    }

    let bytes = response.bytes().await.map_err(|e| {
        let error = CommandError::network(format!("读取响应失败: {}", e));
        if let Some(ref mut s) = span {
            s.finish_error(&error);
        }
        error
    })?;

    if let Some(ref s) = span {
        s.info(
            format!("下载完成，大小: {} bytes", bytes.len()),
            Some(serde_json::json!({ "size": bytes.len() }))
        );
    }

    // 步骤 2: 保存临时文件
    let temp_path = dest.join("temp.zip");
    let write_result = fs::write(&temp_path, &bytes);
    if let Some(ref s) = span {
        s.fs_op("write", &temp_path, &write_result);
    }
    write_result?;

    // 步骤 3: 创建解压目录
    let extract_path = dest.join("extracted");
    let mkdir_result = fs::create_dir_all(&extract_path);
    if let Some(ref s) = span {
        s.fs_op("create_dir", &extract_path, &mkdir_result);
    }
    mkdir_result?;

    // 步骤 4: 解压文件
    if let Some(ref s) = span {
        s.info("开始解压文件", None);
    }

    let extract_result = extract_zip(&temp_path, &extract_path);
    if let Some(ref s) = span {
        s.io("extract_zip", &extract_result);
    }
    extract_result?;

    // 步骤 5: 清理临时文件
    let remove_result = fs::remove_file(&temp_path);
    if let Some(ref s) = span {
        s.fs_op("remove_file", &temp_path, &remove_result);
    }
    remove_result?;

    // 完成
    if let Some(ref mut s) = span {
        s.finish_ok(Some(serde_json::json!({
            "url": url,
            "extract_path": extract_path.to_string_lossy(),
            "file_size": bytes.len(),
        })));
    }

    Ok(extract_path)
}

// ============================================================
// 示例 3: 使用 finish_span 简化代码
// ============================================================

// 更简洁的版本，使用辅助函数
pub async fn download_file_simplified(url: &str, dest: &Path) -> CommandResult<PathBuf> {
    let span = global().map(|logger| logger.operation("downloader", "download_file"));

    if let Some(ref s) = span {
        s.info(format!("下载文件: {}", url), Some(serde_json::json!({ "url": url })));
    }

    let result = async {
        let response = reqwest::get(url).await?;
        let status = response.status().as_u16();

        if let Some(ref s) = span {
            s.http_request("GET", url, Some(status));
        }

        if !response.status().is_success() {
            return Err(CommandError::network(format!("HTTP 错误: {}", status)));
        }

        let bytes = response.bytes().await?;
        let file_path = dest.join("downloaded_file");

        fs::write(&file_path, &bytes)?;

        if let Some(ref s) = span {
            s.fs_op("write", &file_path, &Ok::<_, std::io::Error>(()));
        }

        Ok(file_path)
    }
    .await;

    finish_span(span, result)
}

// ============================================================
// 示例 4: 长时间运行的任务，记录进度
// ============================================================

pub fn batch_process_files(files: &[PathBuf]) -> CommandResult<usize> {
    let mut span = global().map(|logger| logger.operation("batch_processor", "batch_process_files"));

    if let Some(ref s) = span {
        s.info(
            format!("开始批处理 {} 个文件", files.len()),
            Some(serde_json::json!({ "total": files.len() }))
        );
    }

    let mut processed = 0;
    let mut errors = 0;

    for (i, file) in files.iter().enumerate() {
        // 每处理 100 个文件记录一次进度
        if i > 0 && i % 100 == 0 {
            if let Some(ref s) = span {
                s.info(
                    format!("进度: {}/{} ({:.1}%)", i, files.len(), (i as f64 / files.len() as f64) * 100.0),
                    Some(serde_json::json!({
                        "processed": i,
                        "total": files.len(),
                        "errors": errors,
                    }))
                );
            }
        }

        match process_single_file(file) {
            Ok(_) => processed += 1,
            Err(e) => {
                errors += 1;
                if let Some(ref s) = span {
                    s.warn(
                        format!("处理文件失败: {} - {}", file.display(), e.message),
                        Some(serde_json::json!({
                            "file": file.to_string_lossy(),
                            "error": e.code,
                        }))
                    );
                }
            }
        }
    }

    if let Some(ref mut s) = span {
        s.finish_ok(Some(serde_json::json!({
            "total": files.len(),
            "processed": processed,
            "errors": errors,
        })));
    }

    Ok(processed)
}

// ============================================================
// 示例 5: 错误恢复和回退
// ============================================================

pub fn load_with_fallback_and_logging(path: &Path) -> CommandResult<Config> {
    let mut span = global().map(|logger| logger.operation("config", "load_with_fallback"));

    // 尝试从主配置文件加载
    let primary_result = fs::read_to_string(path);

    if let Some(ref s) = span {
        s.fs_op("read", path, &primary_result);
    }

    match primary_result {
        Ok(content) => {
            match serde_json::from_str::<Config>(&content) {
                Ok(config) => {
                    if let Some(ref mut s) = span {
                        s.finish_ok(Some(serde_json::json!({
                            "source": "primary",
                            "path": path.to_string_lossy(),
                        })));
                    }
                    return Ok(config);
                }
                Err(e) => {
                    if let Some(ref s) = span {
                        s.warn(
                            format!("主配置文件格式错误: {}", e),
                            Some(serde_json::json!({ "error": e.to_string() }))
                        );
                    }
                }
            }
        }
        Err(e) => {
            if let Some(ref s) = span {
                s.warn(
                    format!("无法读取主配置文件: {}", e),
                    Some(serde_json::json!({ "path": path.to_string_lossy() }))
                );
            }
        }
    }

    // 尝试从备份加载
    let backup_path = path.with_extension("bak");
    if let Some(ref s) = span {
        s.info("尝试从备份文件加载", Some(serde_json::json!({
            "backup_path": backup_path.to_string_lossy()
        })));
    }

    let backup_result = fs::read_to_string(&backup_path);
    if let Some(ref s) = span {
        s.fs_op("read", &backup_path, &backup_result);
    }

    if let Ok(content) = backup_result {
        if let Ok(config) = serde_json::from_str::<Config>(&content) {
            if let Some(ref mut s) = span {
                s.finish_ok(Some(serde_json::json!({
                    "source": "backup",
                    "path": backup_path.to_string_lossy(),
                })));
            }
            return Ok(config);
        }
    }

    // 使用默认配置
    if let Some(ref s) = span {
        s.warn("所有配置源失败，使用默认配置", None);
    }

    if let Some(ref mut s) = span {
        s.finish_ok(Some(serde_json::json!({ "source": "default" })));
    }

    Ok(Config::default())
}

// ============================================================
// 辅助类型和函数
// ============================================================

#[derive(serde::Deserialize, serde::Serialize)]
struct Config {
    // 配置字段
}

impl Default for Config {
    fn default() -> Self {
        Config {}
    }
}

fn extract_zip(source: &Path, dest: &Path) -> CommandResult<()> {
    // 模拟解压操作
    Ok(())
}

fn process_single_file(path: &Path) -> CommandResult<()> {
    // 模拟文件处理
    Ok(())
}
