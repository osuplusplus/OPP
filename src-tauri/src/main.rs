// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 更新助手只执行文件替换，不初始化窗口、托盘或单实例插件。
    if opp_lib::run_portable_update_helper_if_requested() {
        return;
    }
    opp_lib::run()
}
