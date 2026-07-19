//! exe 리소스 임베드(아이콘 + 버전정보) — 외부 crate 0(DR-8) 유지를 위해
//! Windows SDK `rc.exe`를 직접 호출해 `.res`를 만들고 링커에 넘긴다.
//! rc.exe를 못 찾으면 **경고 후 스킵**(빌드 실패 없음 — 아이콘만 빠진 exe).
//!
//! - 아이콘: `assets/nexa-dir.ico`(리소스 ID 1 = 탐색기/작업표시줄 표시 아이콘).
//! - 버전정보: 배포명 **"Nexa Dir"**(ProductName·FileDescription) — 내부 프로젝트는
//!   nexa-dir2지만 외부 배포는 Nexa Dir(사용자 확정).

use std::path::{Path, PathBuf};
use std::{env, fs, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/nexa-dir.ico");

    // Windows 타깃에서만 리소스 임베드(맥 cross-check 등은 스킵).
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ico = manifest.join("assets/nexa-dir.ico");
    if !ico.exists() {
        println!("cargo:warning=nexa-dir.ico 없음 — exe 아이콘 임베드 스킵");
        return;
    }

    let Some(rc) = find_rc() else {
        println!(
            "cargo:warning=rc.exe(Windows SDK) 미발견 — exe 리소스 아이콘 임베드 스킵\
             (설치 후 작업표시줄 고정 시 빈 아이콘일 수 있음)"
        );
        return;
    };

    // 버전 = Cargo 패키지 버전(x.y.z → x,y,z,0).
    let ver = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut p = ver.split('.').map(|s| s.parse::<u16>().unwrap_or(0));
    let (v0, v1, v2) = (p.next().unwrap_or(0), p.next().unwrap_or(0), p.next().unwrap_or(0));

    // .rc 생성(ico 절대경로 — 백슬래시 이스케이프).
    let ico_esc = ico.to_string_lossy().replace('\\', "\\\\");
    let rc_src = format!(
        r#"1 ICON "{ico}"
1 VERSIONINFO
FILEVERSION {v0},{v1},{v2},0
PRODUCTVERSION {v0},{v1},{v2},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "SosomLab"
      VALUE "FileDescription", "Nexa Dir"
      VALUE "FileVersion", "{v0}.{v1}.{v2}.0"
      VALUE "InternalName", "nexa-dir"
      VALUE "OriginalFilename", "NexaDir.exe"
      VALUE "ProductName", "Nexa Dir"
      VALUE "ProductVersion", "{v0}.{v1}.{v2}.0"
      VALUE "LegalCopyright", "(C) SosomLab"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        ico = ico_esc,
        v0 = v0,
        v1 = v1,
        v2 = v2,
    );
    let rc_path = out_dir.join("nexa-app.rc");
    let res_path = out_dir.join("nexa-app.res");
    fs::write(&rc_path, rc_src).expect("write .rc");

    // rc.exe /fo <res> <rc>
    let status = Command::new(&rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res_path)
        .arg(&rc_path)
        .status();
    match status {
        Ok(s) if s.success() && res_path.exists() => {
            println!("cargo:rustc-link-arg={}", res_path.display());
        }
        _ => println!("cargo:warning=rc.exe 컴파일 실패 — exe 아이콘 임베드 스킵"),
    }
}

/// rc.exe 탐색: PATH → Windows Kits 10 최신 버전(x64).
fn find_rc() -> Option<PathBuf> {
    // 1) PATH
    if let Ok(path) = env::var("PATH") {
        for dir in env::split_paths(&path) {
            let cand = dir.join("rc.exe");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    // 2) Windows Kits 10\bin\<버전>\x64\rc.exe (최신 버전 선택)
    for pf in ["ProgramFiles(x86)", "ProgramFiles"] {
        let Ok(base) = env::var(pf) else { continue };
        let bin = Path::new(&base).join("Windows Kits").join("10").join("bin");
        let Ok(entries) = fs::read_dir(&bin) else { continue };
        let mut vers: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("x64").join("rc.exe").is_file())
            .collect();
        vers.sort();
        if let Some(latest) = vers.last() {
            return Some(latest.join("x64").join("rc.exe"));
        }
    }
    None
}
