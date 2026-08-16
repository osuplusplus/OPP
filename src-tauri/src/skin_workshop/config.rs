use std::{
    fs,
    path::{Path, PathBuf},
};

use walkdir::WalkDir;

use crate::{
    error::{CommandError, CommandResult},
    local_analysis::parser::decode_text,
};

use super::models::{SkinConfigDocument, SkinConfigEntry, SkinConfigError, SkinConfigSection};

pub(crate) fn copy_directory(source: &Path, target: &Path) -> CommandResult<()> {
    fs::create_dir_all(target)?;
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry
            .map_err(|error| CommandError::new("SKIN_WORKSHOP_COPY_ERROR", error.to_string()))?;
        if entry.path_is_symlink() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| CommandError::new("SKIN_WORKSHOP_COPY_ERROR", error.to_string()))?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(destination)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

fn find_skin_ini(root: &Path) -> Option<PathBuf> {
    fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case("skin.ini"))
        })
        .map(|entry| entry.path())
}

pub(crate) fn read_config(root: &Path) -> CommandResult<SkinConfigDocument> {
    let path = find_skin_ini(root)
        .ok_or_else(|| CommandError::new("SKIN_CONFIG_NOT_FOUND", "未找到 skin.ini 文件"))?;
    let bytes = fs::read(&path).map_err(|error| {
        CommandError::new(
            "SKIN_CONFIG_READ_ERROR",
            format!("无法读取 skin.ini：{error}"),
        )
    })?;
    let source = decode_text(&bytes);
    let (encoding, newline) = format_metadata(&bytes, &source);
    let (sections, errors) = parse_config(&source);
    Ok(SkinConfigDocument {
        source,
        sections,
        errors,
        encoding,
        newline,
    })
}

pub(crate) fn write_config_source(root: &Path, source: &str) -> CommandResult<()> {
    let path = find_skin_ini(root)
        .ok_or_else(|| CommandError::new("SKIN_CONFIG_NOT_FOUND", "未找到 skin.ini 文件"))?;
    let original = fs::read(&path).unwrap_or_default();
    let bytes = encode_like(&original, source);
    let temporary = root.join("skin.ini.opp-workshop");
    fs::write(&temporary, bytes)?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}

pub(crate) fn update_config_entry(
    root: &Path,
    section: &str,
    key: &str,
    occurrence: usize,
    value: &str,
) -> CommandResult<()> {
    let document = read_config(root)?;
    if !document.errors.is_empty() {
        return Err(CommandError::new(
            "SKIN_CONFIG_INVALID",
            "请先修复 skin.ini 源码错误",
        ));
    }
    let newline = if document.newline == "crlf" {
        "\r\n"
    } else {
        "\n"
    };
    let trailing_newline = document.source.ends_with('\n');
    let mut current = "Root".to_string();
    let mut seen = 0usize;
    let mut changed = false;
    let mut lines = document
        .source
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    for line in &mut lines {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current = trimmed[1..trimmed.len() - 1].trim().to_string();
            continue;
        }
        if current.eq_ignore_ascii_case(section)
            && let Some((left, _)) = line.split_once(':')
            && left.trim().eq_ignore_ascii_case(key)
        {
            if seen == occurrence {
                let colon = line.find(':').unwrap_or(line.len());
                let prefix = &line[..=colon];
                *line = format!("{prefix} {value}");
                changed = true;
                break;
            }
            seen += 1;
        }
    }
    if !changed {
        return Err(CommandError::new(
            "SKIN_CONFIG_ENTRY_NOT_FOUND",
            "未找到要修改的配置项",
        ));
    }
    let mut source = lines.join(newline);
    if trailing_newline {
        source.push_str(newline);
    }
    write_config_source(root, &source)
}

pub(crate) fn upsert_config_entry(
    root: &Path,
    section: &str,
    key: &str,
    occurrence: usize,
    value: &str,
) -> CommandResult<()> {
    match update_config_entry(root, section, key, occurrence, value) {
        Ok(()) => return Ok(()),
        Err(error) if error.code != "SKIN_CONFIG_ENTRY_NOT_FOUND" => return Err(error),
        Err(_) => {}
    }

    let document = read_config(root)?;
    if !document.errors.is_empty() {
        return Err(CommandError::new(
            "SKIN_CONFIG_INVALID",
            "请先修复 skin.ini 源码错误",
        ));
    }
    let newline = if document.newline == "crlf" {
        "\r\n"
    } else {
        "\n"
    };
    let trailing_newline = document.source.ends_with('\n');
    let mut lines = document
        .source
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let mut section_start = None;
    let mut section_end = lines.len();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let name = trimmed[1..trimmed.len() - 1].trim();
            if section_start.is_some() {
                section_end = index;
                break;
            }
            if name.eq_ignore_ascii_case(section) {
                section_start = Some(index);
            }
        }
    }
    if section_start.is_some() {
        lines.insert(section_end, format!("{key}: {value}"));
    } else {
        if !lines.is_empty() && !lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push(format!("[{section}]"));
        lines.push(format!("{key}: {value}"));
    }
    let mut source = lines.join(newline);
    if trailing_newline || !source.is_empty() {
        source.push_str(newline);
    }
    write_config_source(root, &source)
}

pub(crate) fn replace_config_sections(
    target_root: &Path,
    source_root: &Path,
    section_name: &str,
) -> CommandResult<()> {
    let target = read_config(target_root)?;
    let source = read_config(source_root)?;
    if !target.errors.is_empty() || !source.errors.is_empty() {
        return Err(CommandError::new(
            "SKIN_CONFIG_INVALID",
            "来源或目标 skin.ini 存在语法错误",
        ));
    }
    let newline = if target.newline == "crlf" {
        "\r\n"
    } else {
        "\n"
    };
    let collect = |text: &str| {
        let lines = text.lines().collect::<Vec<_>>();
        let mut blocks = Vec::<Vec<String>>::new();
        let mut current: Option<Vec<String>> = None;
        for line in lines {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                if let Some(block) = current.take() {
                    blocks.push(block);
                }
                let name = trimmed[1..trimmed.len() - 1].trim();
                current = name
                    .eq_ignore_ascii_case(section_name)
                    .then(|| vec![line.to_string()]);
            } else if let Some(block) = &mut current {
                block.push(line.to_string());
            }
        }
        if let Some(block) = current {
            blocks.push(block);
        }
        blocks
    };
    let source_blocks = collect(&source.source);
    if source_blocks.is_empty() {
        return Err(CommandError::new(
            "SKIN_PRESET_SOURCE_MISSING",
            format!("来源 Skin 没有 [{section_name}] 配置节"),
        ));
    }
    let target_lines = target.source.lines().collect::<Vec<_>>();
    let mut kept = Vec::<String>::new();
    let mut skipping = false;
    for line in target_lines {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let name = trimmed[1..trimmed.len() - 1].trim();
            skipping = name.eq_ignore_ascii_case(section_name);
        }
        if !skipping {
            kept.push(line.to_string());
        }
    }
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    if !kept.is_empty() {
        kept.push(String::new());
    }
    for (index, block) in source_blocks.into_iter().enumerate() {
        if index > 0 {
            kept.push(String::new());
        }
        kept.extend(block);
    }
    let mut output = kept.join(newline);
    output.push_str(newline);
    write_config_source(target_root, &output)
}

pub(crate) fn set_skin_name(root: &Path, name: &str) -> CommandResult<()> {
    let document = read_config(root)?;
    if !document.errors.is_empty() {
        return Err(CommandError::new(
            "SKIN_CONFIG_INVALID",
            "skin.ini 存在语法错误，无法发布",
        ));
    }
    if let Some(section) = document
        .sections
        .iter()
        .find(|item| item.name.eq_ignore_ascii_case("General"))
        && let Some(entry) = section
            .entries
            .iter()
            .find(|item| item.key.eq_ignore_ascii_case("Name"))
    {
        return update_config_entry(root, "General", "Name", entry.occurrence, name);
    }
    let newline = if document.newline == "crlf" {
        "\r\n"
    } else {
        "\n"
    };
    let mut source = document.source;
    if !source.is_empty() && !source.ends_with('\n') {
        source.push_str(newline);
    }
    source.push_str(&format!("[General]{newline}Name: {name}{newline}"));
    write_config_source(root, &source)
}

fn parse_config(source: &str) -> (Vec<SkinConfigSection>, Vec<SkinConfigError>) {
    let mut sections = Vec::<SkinConfigSection>::new();
    let mut current = SkinConfigSection {
        name: "Root".into(),
        entries: Vec::new(),
    };
    let mut errors = Vec::new();
    let mut occurrences = std::collections::BTreeMap::<(String, String), usize>::new();
    for (index, raw) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') || line.len() < 3 {
                errors.push(SkinConfigError {
                    line: line_number,
                    message: "配置节缺少右方括号".into(),
                });
                continue;
            }
            if !current.entries.is_empty() || current.name != "Root" {
                sections.push(current);
            }
            current = SkinConfigSection {
                name: line[1..line.len() - 1].trim().to_string(),
                entries: Vec::new(),
            };
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            errors.push(SkinConfigError {
                line: line_number,
                message: "配置项必须使用 key: value 格式".into(),
            });
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            errors.push(SkinConfigError {
                line: line_number,
                message: "配置键不能为空".into(),
            });
            continue;
        }
        let occurrence_key = (current.name.to_ascii_lowercase(), key.to_ascii_lowercase());
        let occurrence = occurrences.entry(occurrence_key).or_default();
        current.entries.push(SkinConfigEntry {
            key: key.to_string(),
            value: value.trim().to_string(),
            occurrence: *occurrence,
            line: line_number,
        });
        *occurrence += 1;
    }
    if !current.entries.is_empty() || current.name != "Root" {
        sections.push(current);
    }
    if sections.is_empty() {
        errors.push(SkinConfigError {
            line: 1,
            message: "skin.ini 至少需要一个配置节".into(),
        });
    }
    (sections, errors)
}

fn format_metadata(bytes: &[u8], source: &str) -> (String, String) {
    let encoding = if bytes.starts_with(&[0xff, 0xfe]) {
        "utf-16le"
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        "utf-16be"
    } else if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        "utf-8-bom"
    } else {
        "utf-8"
    };
    let newline = if source.contains("\r\n") {
        "crlf"
    } else {
        "lf"
    };
    (encoding.into(), newline.into())
}

fn encode_like(original: &[u8], source: &str) -> Vec<u8> {
    if original.starts_with(&[0xff, 0xfe]) {
        let mut bytes = vec![0xff, 0xfe];
        for value in source.encode_utf16() {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    } else if original.starts_with(&[0xfe, 0xff]) {
        let mut bytes = vec![0xfe, 0xff];
        for value in source.encode_utf16() {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes
    } else if original.starts_with(&[0xef, 0xbb, 0xbf]) {
        let mut bytes = vec![0xef, 0xbb, 0xbf];
        bytes.extend_from_slice(source.as_bytes());
        bytes
    } else {
        source.as_bytes().to_vec()
    }
}

pub(crate) fn safe_child(root: &Path, logical_path: &str) -> CommandResult<PathBuf> {
    let relative = Path::new(logical_path);
    if relative.is_absolute()
        || relative.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(CommandError::new(
            "SKIN_WORKSHOP_PATH_INVALID",
            "资源路径不合法",
        ));
    }
    Ok(root.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn config_parser_preserves_duplicates_and_reports_bad_lines() {
        let (sections, errors) = parse_config("[General]\r\nName: A\r\nName: B\r\nbroken\r\n");
        assert_eq!(sections[0].entries[1].occurrence, 1);
        assert_eq!(errors[0].line, 4);
    }

    #[test]
    fn structured_edit_keeps_utf16_and_crlf() {
        let root = tempdir().expect("root");
        let source = "[General]\r\nName: Before\r\n// comment\r\n";
        let mut bytes = vec![0xff, 0xfe];
        for value in source.encode_utf16() {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        fs::write(root.path().join("skin.ini"), bytes).expect("config");
        update_config_entry(root.path(), "General", "Name", 0, "After").expect("edit");
        let document = read_config(root.path()).expect("read");
        assert_eq!(document.encoding, "utf-16le");
        assert_eq!(document.newline, "crlf");
        assert!(document.source.contains("Name: After"));
        assert!(document.source.contains("// comment"));
    }

    #[test]
    fn upsert_adds_missing_entries_without_dropping_existing_config() {
        let root = tempdir().expect("root");
        fs::write(
            root.path().join("skin.ini"),
            "[General]\r\nName: Before\r\n\r\n[Colours]\r\nCombo1: 1,2,3\r\n",
        )
        .expect("config");
        upsert_config_entry(root.path(), "General", "AnimationFramerate", 0, "24")
            .expect("upsert existing section");
        upsert_config_entry(root.path(), "Fonts", "HitCirclePrefix", 0, "default")
            .expect("upsert new section");
        let document = read_config(root.path()).expect("read");
        assert_eq!(document.newline, "crlf");
        assert!(document.source.contains("AnimationFramerate: 24"));
        assert!(document.source.contains("[Colours]\r\nCombo1: 1,2,3"));
        assert!(
            document
                .source
                .contains("[Fonts]\r\nHitCirclePrefix: default")
        );
    }
}
