//! Desktop data-directory policy separating development and packaged releases.

use std::path::{Path, PathBuf};

const DATA_ROOT: &str = ".topic-desk-studio";

/// Debug builds use isolated data so local development cannot mutate a user's
/// packaged application database. Release builds always use the production tree.
#[cfg(debug_assertions)]
const RUNTIME_DIRECTORY: &str = "dev";
#[cfg(not(debug_assertions))]
const RUNTIME_DIRECTORY: &str = "prod";

/// Resolve and create the private directory used by every durable app service.
pub fn prepare(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;

    let home = app
        .path()
        .home_dir()
        .map_err(|error| format!("无法解析用户主目录：{error}"))?;
    let root = home.join(DATA_ROOT);
    let directory = directory_for(&home, RUNTIME_DIRECTORY);
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("无法创建应用数据目录：{error}"))?;
    restrict_to_current_user(&root)?;
    restrict_to_current_user(&directory)?;
    Ok(directory)
}

/// Keep path selection testable without depending on a running Tauri context.
fn directory_for(home: &Path, runtime: &str) -> PathBuf {
    home.join(DATA_ROOT).join(runtime)
}

#[cfg(unix)]
fn restrict_to_current_user(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("无法限制应用数据目录权限：{error}"))
}

#[cfg(not(unix))]
fn restrict_to_current_user(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_development_and_production_directories() {
        let home = Path::new("/users/example");
        assert_eq!(
            directory_for(home, "dev"),
            home.join(".topic-desk-studio/dev")
        );
        assert_eq!(
            directory_for(home, "prod"),
            home.join(".topic-desk-studio/prod")
        );
    }
}
