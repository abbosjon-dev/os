//! Yadro yiğilişidan oldin `user/` dagi foydalanuvçi dasturlarini yiğadi va
//! ularni yadro içiga joylaydi (`include_bytes!`). Fayl tizimi paydo bölgunça
//! dasturlar şu tarzda yadro bilan birga keladi.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const TARGET: &str = "aarch64-unknown-none-softfloat";

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let user = root.join("user");
    let target_dir = root.join("target").join("user");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed=user/src");
    println!("cargo:rerun-if-changed=user/Cargo.toml");
    println!("cargo:rerun-if-changed=user/user.ld");
    println!("cargo:rerun-if-changed=src/abi.rs");

    let flags = [
        format!("-Clink-arg=-T{}", user.join("user.ld").display()),
        "-Crelocation-model=static".to_string(),
    ]
    .join("\x1f");

    let status = Command::new(env::var("CARGO").unwrap())
        .current_dir(&user)
        .args(["build", "--release", "--bins", "--target", TARGET, "--target-dir"])
        .arg(&target_dir)
        .env_remove("RUSTFLAGS")
        .env("CARGO_ENCODED_RUSTFLAGS", flags)
        .status()
        .expect("cargo işga tuşmadi");
    assert!(status.success(), "foydalanuvçi dasturlarini yiğib bölmadi");

    let mut names: Vec<String> = fs::read_dir(user.join("src").join("bin"))
        .unwrap()
        .filter_map(|e| {
            let p = e.ok()?.path();
            if p.extension()? != "rs" {
                return None;
            }
            Some(p.file_stem()?.to_str()?.to_string())
        })
        .collect();
    names.sort();

    let bin_dir = target_dir.join(TARGET).join("release");
    let mut code = String::from("pub static PROGRAMS: &[(&str, &[u8])] = &[\n");
    for n in &names {
        code += &format!("    ({:?}, include_bytes!({:?})),\n", n, bin_dir.join(n).display().to_string());
    }
    code += "];\n";
    fs::write(out.join("programs.rs"), code).unwrap();
}
