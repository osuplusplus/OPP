//! 读取时先复制 `client.realm` 快照到临时目录再打开，避免与正在运行的
//! lazer 进程争抢 `.lock` 文件。
//!
//! 加载策略：小表全量载入，大表懒加载

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use realm_db_reader::{Group, Link, Realm, Row, Value};
use serde::{Deserialize, Serialize};

/// 行数不超过该阈值的表整表全载；超过的表按行懒加载。
const BULK_ROW_LIMIT: usize = 50_000;

/// lazer files/ 内容寻址存储中的 blob 相对路径：`x/xy/<sha256>`。
pub fn blob_relative_path(hash: &str) -> String {
    let bytes = hash.as_bytes();
    if bytes.len() >= 2 {
        format!(
            "{}/{}/{}",
            hash[..1].to_ascii_lowercase(),
            hash[..2].to_ascii_lowercase(),
            hash
        )
    } else {
        hash.to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazerRealmFile {
    pub filename: String,
    pub hash: String,
    /// 对应 files/ 内容寻址 blob 的磁盘大小（读取时已 stat，字节级统计直接可用）。
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazerRealmBeatmap {
    /// .osu 文件内容的 SHA-256（与谱面集文件列表中的条目对应）。
    pub sha256: String,
    /// .osu 文件内容的 MD5（收藏夹 / 游戏会话按 MD5 查谱面）。
    pub md5: String,
    pub online_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazerRealmSet {
    pub id: String,
    pub online_id: i64,
    pub artist: String,
    pub artist_unicode: String,
    pub title: String,
    pub title_unicode: String,
    pub creator: String,
    pub beatmaps: Vec<LazerRealmBeatmap>,
    pub files: Vec<LazerRealmFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazerRealmSkin {
    pub id: String,
    pub name: String,
    pub creator: String,
    /// skin.ini 内容的 SHA-256；内置皮肤没有文件列表，此值为 None。
    pub skin_ini: Option<LazerRealmFile>,
    pub files: Vec<LazerRealmFile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LazerRealmData {
    pub table_count: usize,
    pub sets: Vec<LazerRealmSet>,
    pub skins: Vec<LazerRealmSkin>,
}

/// 读取（快照后的）Realm 数据。`realm_path` 指向 lazer 数据目录里的 client.realm。
/// 文件条目会附带 files/ 中对应 blob 的大小。
pub fn read_realm_data(realm_path: &Path) -> Result<LazerRealmData, String> {
    let snapshot = snapshot_realm(realm_path)?;
    let result = parse_realm(&snapshot);
    let _ = std::fs::remove_file(&snapshot);
    let mut data = result?;
    let files_root = realm_path
        .parent()
        .map(|parent| parent.join("files"))
        .unwrap_or_else(|| PathBuf::from("files"));
    for file in data
        .sets
        .iter_mut()
        .flat_map(|set| set.files.iter_mut())
        .chain(data.skins.iter_mut().flat_map(|skin| skin.files.iter_mut()))
    {
        file.size = fs_metadata_size(&files_root, &file.hash);
    }
    Ok(data)
}

/// 复制一份快照用于读取，避免触碰 lazer 的锁文件。
fn snapshot_realm(realm_path: &Path) -> Result<PathBuf, String> {
    let snapshot =
        std::env::temp_dir().join(format!("opp-client-realm-{}.realm", std::process::id()));
    std::fs::copy(realm_path, &snapshot)
        .map_err(|error| format!("复制 client.realm 快照失败：{error}"))?;
    Ok(snapshot)
}

fn fs_metadata_size(files_root: &Path, hash: &str) -> u64 {
    std::fs::metadata(files_root.join(blob_relative_path(hash)))
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn parse_realm(snapshot: &Path) -> Result<LazerRealmData, String> {
    let realm =
        Realm::open(snapshot).map_err(|error| format!("打开 client.realm 失败：{error}"))?;
    let group = realm
        .into_group()
        .map_err(|error| format!("读取 Realm 组失败：{error}"))?;
    let mut store = RowStore::new(&group);

    let mut data = LazerRealmData {
        table_count: group.table_count(),
        ..LazerRealmData::default()
    };

    for row in store.bulk_rows("class_BeatmapSet")? {
        if matches!(row.get("DeletePending"), Some(Value::Bool(true))) {
            continue;
        }
        let files = resolve_named_files(&mut store, row.get("Files"))?;
        let mut beatmaps = Vec::new();
        if let Some(Value::LinkList(links)) = row.get("Beatmaps") {
            for link in links {
                let Some(beatmap) = store.row(link)? else {
                    continue;
                };
                let sha256 = string_value(beatmap.get("Hash"));
                if sha256.is_empty() || !files.iter().any(|file| file.hash == sha256) {
                    // 谱面对应的 .osu 不在文件列表里（比如导入中断），跳过。
                    continue;
                }
                beatmaps.push(LazerRealmBeatmap {
                    sha256,
                    md5: string_value(beatmap.get("MD5Hash")),
                    online_id: int_value(beatmap.get("OnlineID"), -1),
                });
            }
        }
        if beatmaps.is_empty() {
            continue;
        }

        // 元数据取第一张难度关联的 BeatmapMetadata（与 lazer 自身行为一致）。
        let metadata = row
            .get("Beatmaps")
            .and_then(first_link)
            .and_then(|link| store.row(link).ok().flatten())
            .and_then(|beatmap| match beatmap.get("Metadata") {
                Some(Value::Link(link)) => Some(link.clone()),
                _ => None,
            })
            .and_then(|link| store.row(&link).ok().flatten());
        let (artist, artist_unicode, title, title_unicode, creator) = match metadata {
            Some(metadata) => {
                let artist = string_value(metadata.get("Artist"));
                let title = string_value(metadata.get("Title"));
                // Unicode 字段为空时回退罗马字（lazer 的常见存储行为）。
                (
                    artist.clone(),
                    non_empty_or(string_value(metadata.get("ArtistUnicode")), artist),
                    title.clone(),
                    non_empty_or(string_value(metadata.get("TitleUnicode")), title),
                    match metadata.get("Author") {
                        Some(Value::Link(user_link)) => store
                            .row(user_link)
                            .ok()
                            .flatten()
                            .map(|user| string_value(user.get("Username")))
                            .unwrap_or_default(),
                        _ => String::new(),
                    },
                )
            }
            None => (
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ),
        };

        data.sets.push(LazerRealmSet {
            id: uuid_string(row.get("ID")),
            online_id: int_value(row.get("OnlineID"), -1),
            artist,
            artist_unicode,
            title,
            title_unicode,
            creator,
            beatmaps,
            files,
        });
    }

    for row in store.bulk_rows("class_Skin")? {
        if matches!(row.get("DeletePending"), Some(Value::Bool(true))) {
            continue;
        }
        let files = resolve_named_files(&mut store, row.get("Files"))?;
        let skin_ini = files
            .iter()
            .find(|file| file.filename.eq_ignore_ascii_case("skin.ini"))
            .cloned();
        if skin_ini.is_none() {
            // 内置皮肤（Argon/Triangles 等）没有文件列表，无法提供本地预览。
            continue;
        }
        data.skins.push(LazerRealmSkin {
            id: uuid_string(row.get("ID")),
            name: string_value(row.get("Name")),
            creator: string_value(row.get("Creator")),
            skin_ini,
            files,
        });
    }

    Ok(data)
}

fn non_empty_or(value: String, fallback: String) -> String {
    if value.is_empty() { fallback } else { value }
}

/// 表加载策略：小表整表全载（`Bulk`），大表仅保留表句柄、按行懒加载（`Lazy`）。
enum TableData {
    Bulk(Vec<Row<'static>>),
    Lazy {
        table: realm_db_reader::Table,
        rows: HashMap<usize, Row<'static>>,
    },
}

/// 按表名缓存已加载的表；行访问统一走 [`RowStore::row`]，
/// 大表的行只有在被链接真正指向时才会加载。
struct RowStore<'a> {
    group: &'a Group,
    numbers: HashMap<String, usize>,
    tables: HashMap<usize, TableData>,
}

impl<'a> RowStore<'a> {
    fn new(group: &'a Group) -> Self {
        Self {
            group,
            numbers: HashMap::new(),
            tables: HashMap::new(),
        }
    }

    fn table_number(&mut self, name: &str) -> Result<usize, String> {
        if let Some(number) = self.numbers.get(name) {
            return Ok(*number);
        }
        let number = self
            .group
            .get_table_names()
            .iter()
            .position(|table_name| table_name == name)
            .ok_or_else(|| format!("数据库中没有 {name} 表"))?;
        self.numbers.insert(name.to_string(), number);
        Ok(number)
    }

    fn prepare(&mut self, name: &str) -> Result<usize, String> {
        let number = self.table_number(name)?;
        if self.tables.contains_key(&number) {
            return Ok(number);
        }
        let table = self
            .group
            .get_table(number)
            .map_err(|error| error.to_string())?;
        let data = if table.row_count().unwrap_or(0) <= BULK_ROW_LIMIT {
            let rows: Vec<Row<'static>> = table
                .get_rows()
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(Row::into_owned)
                .collect();
            TableData::Bulk(rows)
        } else {
            TableData::Lazy {
                table,
                rows: HashMap::new(),
            }
        };
        self.tables.insert(number, data);
        Ok(number)
    }

    /// 全量载入一张（小）表；仅用于需要整表遍历的表。
    fn bulk_rows(&mut self, name: &str) -> Result<Vec<Row<'static>>, String> {
        let number = self.prepare(name)?;
        match self.tables.get(&number).expect("刚刚插入的表一定存在") {
            TableData::Bulk(rows) => Ok(rows.clone()),
            // 大表不提供整表遍历：调用方只应遍历小表。
            TableData::Lazy { .. } => Err(format!("{name} 行数过多，不支持整表载入")),
        }
    }

    /// 解引用一条链接；大表按需加载单行并缓存，小表直接取已载入的行。
    fn row(&mut self, link: &Link) -> Result<Option<Row<'static>>, String> {
        // 链接只带表号，懒加载的表可能尚未 prepare：按表号补建。
        if !self.tables.contains_key(&link.target_table_number) {
            let table = self
                .group
                .get_table(link.target_table_number)
                .map_err(|error| error.to_string())?;
            let data = if table.row_count().unwrap_or(0) <= BULK_ROW_LIMIT {
                let rows: Vec<Row<'static>> = table
                    .get_rows()
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .map(Row::into_owned)
                    .collect();
                TableData::Bulk(rows)
            } else {
                TableData::Lazy {
                    table,
                    rows: HashMap::new(),
                }
            };
            self.tables.insert(link.target_table_number, data);
        }
        let data = self
            .tables
            .get_mut(&link.target_table_number)
            .expect("上面已确保存在");
        match data {
            TableData::Bulk(rows) => Ok(rows.get(link.row_number).cloned()),
            TableData::Lazy { table, rows } => {
                if let Some(row) = rows.get(&link.row_number) {
                    return Ok(Some(row.clone()));
                }
                let row = table
                    .get_row(link.row_number)
                    .map_err(|error| error.to_string())?
                    .into_owned();
                rows.insert(link.row_number, row.clone());
                Ok(Some(row))
            }
        }
    }
}

fn first_link(value: &Value) -> Option<&Link> {
    match value {
        Value::LinkList(links) => links.first(),
        _ => None,
    }
}

fn string_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        _ => String::new(),
    }
}

fn int_value(value: Option<&Value>, fallback: i64) -> i64 {
    match value {
        Some(Value::Int(value)) => *value,
        _ => fallback,
    }
}

fn uuid_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::Uuid(bytes)) => bytes.iter().map(|b| format!("{b:02x}")).collect(),
        _ => String::new(),
    }
}

/// 展开 BeatmapSet/Skin 行的 Files 链接列表为 (filename, hash)。
/// `RealmNamedFileUsage` 与 `File` 都是大表，全部按行懒加载。
fn resolve_named_files(
    store: &mut RowStore<'_>,
    value: Option<&Value>,
) -> Result<Vec<LazerRealmFile>, String> {
    let Some(Value::LinkList(links)) = value else {
        return Ok(Vec::new());
    };
    let mut files = Vec::with_capacity(links.len());
    for link in links {
        let Some(usage) = store.row(link)? else {
            continue;
        };
        let filename = string_value(usage.get("Filename"));
        if filename.is_empty() {
            continue;
        }
        let hash = match usage.get("File") {
            Some(Value::Link(file_link)) => {
                let file_row = store.row(file_link)?;
                match file_row {
                    Some(row) => string_value(row.get("Hash")),
                    None => continue,
                }
            }
            _ => continue,
        };
        files.push(LazerRealmFile {
            filename,
            hash,
            size: 0,
        });
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 对真实 lazer 数据库的读取测试：
    /// `cargo test --lib lazer_realm -- --ignored`（需要本地存在 lazer）。
    #[test]
    #[ignore = "需要本机安装 osu!lazer 且存在 client.realm"]
    fn reads_real_lazer_realm() {
        let started = std::time::Instant::now();
        let data_root = crate::platform::lazer_data_root().expect("未找到 lazer 数据目录");
        let data =
            read_realm_data(&data_root.join("client.realm")).expect("读取 client.realm 失败");
        eprintln!(
            "realm read: {} sets, {} skins, {:?}",
            data.sets.len(),
            data.skins.len(),
            started.elapsed()
        );

        assert!(!data.sets.is_empty(), "谱面集为空");
        let sample = data
            .sets
            .iter()
            .find(|set| !set.beatmaps.is_empty())
            .expect("没有可用谱面集");
        assert!(!sample.title.is_empty() || !sample.artist.is_empty());
        let beatmap = &sample.beatmaps[0];
        assert_eq!(beatmap.sha256.len(), 64);
        assert_eq!(beatmap.md5.len(), 32);
        assert!(sample.files.iter().any(|file| file.hash == beatmap.sha256));
        assert!(sample.files.iter().all(|file| file.size > 0));

        let blob = blob_relative_path(&beatmap.sha256);
        let files_root = data_root.join("files");
        assert!(files_root.join(&blob).is_file(), "blob 不存在：{blob}");
    }
}
