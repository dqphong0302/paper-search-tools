//! Local skill installation. Never executes imported content or follows symlinks.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const MARKER: &str = ".scholargateway-install.json";
const DISABLED: &str = "SKILL.md.disabled";
const MAX_BYTES: usize = 20 * 1024 * 1024;
// Serialize our own install/toggle/remove operations, including concurrent IPC calls.
static OPERATIONS: std::sync::Mutex<()> = std::sync::Mutex::new(());
type Files = BTreeMap<String, Vec<u8>>;

fn bundled_files(source: &str) -> Result<Files, String> {
    let content = match source {
        "builtin:paper-search" => include_str!("../../skills/paper-search/SKILL.md"),
        "builtin:paper-collect" => include_str!("../../skills/paper-collect/SKILL.md"),
        "builtin:research-resume" => include_str!("../../skills/research-resume/SKILL.md"),
        _ => return Err("Unknown bundled skill".into()),
    };
    Ok(BTreeMap::from([(
        "SKILL.md".into(),
        content.as_bytes().to_vec(),
    )]))
}

fn source_files(source: &str) -> Result<(PathBuf, Files), String> {
    if source.starts_with("builtin:") {
        Ok((PathBuf::new(), bundled_files(source)?))
    } else {
        let path = directory(source)?;
        let files = snapshot(&path, false)?;
        Ok((path, files))
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub source: String,
    pub path: String,
    pub enabled: bool,
    pub files: usize,
    pub bytes: usize,
    pub content: String,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    name: String,
    source: String,
    enabled: bool,
    hashes: BTreeMap<String, String>,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

pub(crate) fn directory(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err("Use an absolute path that contains no ..".into());
    }
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part);
        // A Windows drive/UNC prefix is not a directory until RootDir is added.
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let meta = fs::symlink_metadata(&current).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err("The directory must exist and must not traverse a symlink".into());
        }
    }
    fs::canonicalize(path).map_err(|e| e.to_string())
}

fn snapshot(root: &Path, installed: bool) -> Result<Files, String> {
    fn walk(
        root: &Path,
        path: &Path,
        depth: usize,
        installed: bool,
        files: &mut Files,
        bytes: &mut usize,
    ) -> Result<(), String> {
        if depth > 16 {
            return Err("Skill exceeds the 16-level directory limit".into());
        }
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                return Err("Skill contains a symlink; not installing".into());
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("File name must be valid UTF-8")?
                .to_string();
            #[cfg(not(windows))]
            if relative.contains('\\') {
                return Err("File name contains an unsafe path separator".into());
            }
            #[cfg(windows)]
            let relative = relative.replace('\\', "/");
            if relative == MARKER {
                if installed {
                    continue;
                }
                return Err("Source contains a reserved install-management file".into());
            }
            if meta.is_dir() {
                walk(root, &path, depth + 1, installed, files, bytes)?;
            } else if meta.is_file() {
                if files.len() >= 256 || meta.len() > MAX_BYTES as u64 {
                    return Err("Skill is too large (256 files / 20 MiB)".into());
                }
                let mut data = Vec::new();
                fs::File::open(&path)
                    .map_err(|e| e.to_string())?
                    .take((MAX_BYTES + 1) as u64)
                    .read_to_end(&mut data)
                    .map_err(|e| e.to_string())?;
                *bytes += data.len();
                if *bytes > MAX_BYTES {
                    return Err("Skill exceeds 20 MiB".into());
                }
                files.insert(relative, data);
            } else {
                return Err("A skill may only contain regular files and directories".into());
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(root, root, 0, installed, &mut files, &mut 0)?;
    Ok(files)
}

fn document(files: &Files, enabled: bool) -> Result<String, String> {
    String::from_utf8(
        files
            .get(if enabled { "SKILL.md" } else { DISABLED })
            .ok_or("SKILL.md is missing")?
            .clone(),
    )
    .map_err(|_| "SKILL.md must be valid UTF-8".into())
}

fn name_from_document(content: &str) -> Result<String, String> {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return Err("SKILL.md needs YAML frontmatter".into());
    }
    let mut name = None;
    let mut description = false;
    let mut closed = false;
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if let Some(value) = line.strip_prefix("name:") {
            name = Some(value.trim().trim_matches(['\'', '"']).to_string());
        }
        if let Some(value) = line.strip_prefix("description:") {
            description = !value.trim().is_empty();
        }
    }
    let name = name.ok_or("Frontmatter is missing a name")?;
    if !closed || !description || !valid_name(&name) {
        return Err("Frontmatter needs a lowercase slug name and a description".into());
    }
    Ok(name)
}

fn hashes(files: &Files) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|(name, bytes)| (name.clone(), format!("{:x}", md5::compute(bytes))))
        .collect()
}

fn receipt(path: &Path) -> Result<Receipt, String> {
    let marker = path.join(MARKER);
    let meta =
        fs::symlink_metadata(&marker).map_err(|_| "Not a skill installed by ScholarGateway")?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 100_000 {
        return Err("Invalid receipt".into());
    }
    serde_json::from_slice(&fs::read(marker).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn info(path: &Path, receipt: &Receipt, files: &Files) -> Result<SkillInfo, String> {
    Ok(SkillInfo {
        name: receipt.name.clone(),
        source: receipt.source.clone(),
        path: path.to_string_lossy().into(),
        enabled: receipt.enabled,
        files: files.len(),
        bytes: files.values().map(Vec::len).sum(),
        content: document(files, receipt.enabled)?,
    })
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn preview_skill(source: String) -> Result<SkillInfo, String> {
    let _guard = OPERATIONS
        .lock()
        .map_err(|_| "The skill manager is in a failed state")?;
    let (path, files) = source_files(&source)?;
    if files.contains_key(DISABLED) {
        return Err("Source contains the reserved SKILL.md.disabled file".into());
    }
    let name = name_from_document(&document(&files, true)?)?;
    info(
        &path,
        &Receipt {
            name,
            source,
            enabled: true,
            hashes: BTreeMap::new(),
        },
        &files,
    )
}

#[tauri::command]
pub fn install_skill(source: String, target_root: String) -> Result<SkillInfo, String> {
    let _guard = OPERATIONS
        .lock()
        .map_err(|_| "The skill manager is in a failed state")?;
    let (source_path, files) = source_files(&source)?;
    let root = directory(&target_root)?;
    if files.contains_key(DISABLED) {
        return Err("Source contains a reserved file name".into());
    }
    let name = name_from_document(&document(&files, true)?)?;
    let target = root.join(&name);
    // Atomic directory creation prevents overwriting existing skills, even on repeated clicks.
    fs::create_dir(&target).map_err(|e| format!("Refusing to overwrite an existing skill: {e}"))?;
    let result = (|| {
        for (name, data) in &files {
            let path = target.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            write_new(&path, data)?;
        }
        let receipt = Receipt {
            name,
            source: if source.starts_with("builtin:") {
                source.clone()
            } else {
                source_path.to_string_lossy().into()
            },
            enabled: true,
            hashes: hashes(&files),
        };
        write_new(
            &target.join(MARKER),
            &serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
        )?;
        info(&target, &receipt, &files)
    })();
    // Preserve incomplete files for recovery rather than deleting anything on failure.
    result.map_err(|error: String| {
        format!(
            "{error}. The incomplete install was kept at {}",
            target.display()
        )
    })
}

#[tauri::command]
pub fn list_skills(target_root: String) -> Result<Vec<SkillInfo>, String> {
    let _guard = OPERATIONS
        .lock()
        .map_err(|_| "The skill manager is in a failed state")?;
    let root = directory(&target_root)?;
    let mut result = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(valid_name)
        {
            continue;
        }
        if fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            continue;
        }
        if !path.join(MARKER).exists() {
            continue;
        }
        let receipt = receipt(&path)?;
        result.push(info(&path, &receipt, &snapshot(&path, true)?)?);
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

#[tauri::command]
pub fn remove_skill(target_root: String, name: String) -> Result<String, String> {
    let _guard = OPERATIONS
        .lock()
        .map_err(|_| "The skill manager is in a failed state")?;
    if !valid_name(&name) {
        return Err("Invalid skill name".into());
    }
    let root = directory(&target_root)?;
    let target = directory(root.join(&name).to_str().ok_or("Invalid path")?)?;
    let receipt = receipt(&target)?;
    let files = snapshot(&target, true)?;
    if receipt.name != name || receipt.hashes != hashes(&files) {
        return Err(
            "The skill changed after installation; not removing it automatically so your edits are kept".into(),
        );
    }
    let archive = root.join(format!(".scholargateway-removed-{}", uuid::Uuid::new_v4()));
    fs::rename(&target, &archive).map_err(|e| e.to_string())?;
    Ok(archive.to_string_lossy().into())
}

#[tauri::command]
pub fn set_skill_enabled(
    target_root: String,
    name: String,
    enabled: bool,
) -> Result<SkillInfo, String> {
    let _guard = OPERATIONS
        .lock()
        .map_err(|_| "The skill manager is in a failed state")?;
    if !valid_name(&name) {
        return Err("Invalid skill name".into());
    }
    let root = directory(&target_root)?;
    let target = directory(root.join(&name).to_str().ok_or("Invalid path")?)?;
    let mut receipt = receipt(&target)?;
    let mut files = snapshot(&target, true)?;
    if receipt.name != name || receipt.hashes != hashes(&files) {
        return Err("The skill changed; leaving it untouched".into());
    }
    if receipt.enabled == enabled {
        return info(&target, &receipt, &files);
    }
    let (from, to) = if enabled {
        (DISABLED, "SKILL.md")
    } else {
        ("SKILL.md", DISABLED)
    };
    if target.join(to).exists() {
        return Err("Refusing to overwrite an existing file".into());
    }
    let data = files.remove(from).ok_or("Skill document is missing")?;
    files.insert(to.into(), data);
    receipt.enabled = enabled;
    receipt.hashes = hashes(&files);
    let pending = target.join(format!(".receipt-{}", uuid::Uuid::new_v4()));
    write_new(
        &pending,
        &serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
    )?;
    fs::rename(target.join(from), target.join(to)).map_err(|e| e.to_string())?;
    if let Err(error) = fs::rename(&pending, target.join(MARKER)) {
        let _ = fs::rename(target.join(to), target.join(from));
        return Err(format!("Could not update the receipt: {error}"));
    }
    info(&target, &receipt, &files)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_skills_preview_install_and_preserve_user_edits() {
        let root = std::env::temp_dir().join(format!("sg-bundled-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let target = root.to_str().unwrap().to_string();
        for name in ["paper-search", "paper-collect", "research-resume"] {
            let source = format!("builtin:{name}");
            let preview = preview_skill(source.clone()).unwrap();
            assert_eq!(preview.name, name);
            assert_eq!(preview.files, 1);
            let installed = install_skill(source.clone(), target.clone()).unwrap();
            assert_eq!(installed.content, preview.content);
            assert_eq!(installed.source, source);
            assert!(install_skill(source, target.clone()).is_err());
            set_skill_enabled(target.clone(), name.into(), false).unwrap();
            set_skill_enabled(target.clone(), name.into(), true).unwrap();
        }
        assert_eq!(list_skills(target.clone()).unwrap().len(), 3);
        fs::write(root.join("paper-search/SKILL.md"), "User edits").unwrap();
        assert!(remove_skill(target.clone(), "paper-search".into()).is_err());
        assert_eq!(
            fs::read_to_string(root.join("paper-search/SKILL.md")).unwrap(),
            "User edits"
        );
        assert!(preview_skill("builtin:../unknown".into()).is_err());
        assert!(install_skill("builtin:../unknown".into(), target).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn install_preview_list_and_recoverable_remove() {
        let root = std::env::temp_dir().join(format!("sg-skill-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        // Resolve OS-provided /var -> /private/var aliases before passing to strict path checks.
        let root = fs::canonicalize(root).unwrap();
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        let doc =
            "---\nname: test-skill\ndescription: Test skill\n---\nResearch across disciplines.";
        fs::write(source.join("SKILL.md"), doc).unwrap();
        let source = source.to_str().unwrap().to_string();
        let target = target.to_str().unwrap().to_string();
        assert_eq!(preview_skill(source.clone()).unwrap().name, "test-skill");
        let installed = install_skill(source.clone(), target.clone()).unwrap();
        assert!(install_skill(source.clone(), target.clone()).is_err());
        assert_eq!(list_skills(target.clone()).unwrap().len(), 1);
        assert!(
            !set_skill_enabled(target.clone(), "test-skill".into(), false)
                .unwrap()
                .enabled
        );
        assert!(!Path::new(&installed.path).join("SKILL.md").exists());
        assert!(
            set_skill_enabled(target.clone(), "test-skill".into(), true)
                .unwrap()
                .enabled
        );
        fs::write(Path::new(&installed.path).join("extra.txt"), "user edit").unwrap();
        assert!(remove_skill(target.clone(), installed.name.clone()).is_err());
        fs::remove_file(Path::new(&installed.path).join("extra.txt")).unwrap();
        let archive = remove_skill(target.clone(), installed.name).unwrap();
        assert!(Path::new(&archive).join("SKILL.md").exists());
        assert!(list_skills(target).unwrap().is_empty());
        assert!(!valid_name("../escape"));
        assert!(directory("relative/path").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("source/SKILL.md"), root.join("source/link"))
                .unwrap();
            assert!(preview_skill(source).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
