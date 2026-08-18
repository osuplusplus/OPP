
//下面是danser的default选项
pub fn default_one() -> f64 {
    1.0
}

pub fn default_danser_frame_width() -> u32 {
    1920
}

pub fn default_danser_frame_height() -> u32 {
    1080
}

pub fn default_danser_fps() -> u32 {
    60
}

pub fn default_danser_encoder() -> String {
    "libx264".into()
}

pub fn default_danser_quality() -> u8 {
    14
}

pub fn default_danser_motion_blur_oversample() -> u32 {
    16
}

pub fn default_true() -> bool {
    true
}

pub fn default_danser_settings_profile() -> String {
    "default".into()
}

//下面是 obs 的default选项

pub fn default_tosu_api_base_url() -> String {
    "http://127.0.0.1:24050".into()
}

pub fn default_launch_tosu_lyrics() -> bool {
    true
}

pub fn default_theme_primary() -> String {
    "cyan".into()
}

pub fn default_theme_secondary() -> String {
    "pink".into()
}

pub fn default_theme_mode() -> String {
    "dark".into()
}

pub fn default_preview_volume() -> u8 {
    45
}

pub fn default_cache_limit_mb() -> u32 {
    512
}

pub fn default_open_local_maps_key() -> String {
    "Alt+1".into()
}

pub fn default_open_trainer_key() -> String {
    "Alt+2".into()
}

pub fn default_open_settings_key() -> String {
    "Alt+,".into()
}

pub fn default_similarity_section_range() -> u32 {
    4
}

pub fn default_similarity_results_per_page() -> u32 {
    5
}

pub fn default_obs_websocket_url() -> String {
    "ws://127.0.0.1:4455".into()
}