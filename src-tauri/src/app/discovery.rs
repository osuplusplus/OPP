// OPP涉及到多个程序的寻找 现在按照用户自定义 -> PATH回退来统一封装
// 或者我感觉甚至可以把 objects,realm, 也抽象出来

use std::path::PathBuf;


pub enum ExecutableName {
    Stable,
    Lazer,
    Danser,
    Tosu,
    OBS,
    Ffmpeg,
}

struct Executable {
    name: ExecutableName,
    path: PathBuf,
    argument: Option<String>,
}

impl Executable {
    pub fn resolve(target: Executable) -> Option<PathBuf> {
        match target.name {
            _ => todo!(),
        }
    }
}


