//! Публикация проекта на GitHub через git + GitHub CLI (gh).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub enum PublishEvent {
    Log(String),
    Done(Result<String, String>),
}

#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub git: Option<String>,
    pub gh: Option<String>,
}

pub fn project_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if manifest.join("Cargo.toml").exists() {
        manifest
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

fn run(program: &str, args: &[&str], dir: &Path) -> Result<String, String> {
    let out = command(program)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("Не удалось запустить {program}: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    if out.status.success() {
        Ok(stdout)
    } else {
        let err = if !stderr.trim().is_empty() {
            stderr
        } else {
            stdout
        };
        Err(err.trim().to_string())
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    run("git", args, dir)
}

fn gh(dir: &Path, args: &[&str]) -> Result<String, String> {
    run("gh", args, dir)
}

pub fn tools() -> Tools {
    let dir = project_dir();
    Tools {
        git: run("git", &["--version"], &dir)
            .ok()
            .map(|s| s.trim().to_string()),
        gh: run("gh", &["--version"], &dir)
            .ok()
            .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
    }
}

fn send(tx: &Sender<PublishEvent>, line: impl Into<String>) {
    let _ = tx.send(PublishEvent::Log(line.into()));
}

fn valid_repo_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

pub fn publish(repo: &str, description: &str, private: bool, tx: &Sender<PublishEvent>) {
    let result = publish_inner(repo, description, private, tx);
    let _ = tx.send(PublishEvent::Done(result));
}

fn publish_inner(
    repo: &str,
    description: &str,
    private: bool,
    tx: &Sender<PublishEvent>,
) -> Result<String, String> {
    let dir = project_dir();
    send(tx, format!("Папка проекта: {}", dir.display()));

    if !valid_repo_name(repo) {
        return Err("Имя репозитория может содержать только латиницу, цифры, '-', '_' и '.'".into());
    }

    // 1. git
    if run("git", &["--version"], &dir).is_err() {
        return Err(
            "git не найден. Установите Git for Windows: https://git-scm.com/download/win".into(),
        );
    }

    if !dir.join(".git").exists() {
        send(tx, "git init…");
        git(&dir, &["init", "-b", "main"])
            .or_else(|_| git(&dir, &["init"]).and_then(|_| git(&dir, &["checkout", "-b", "main"])))?;
        send(tx, "Локальный git-репозиторий создан (ветка main).");
    } else {
        send(tx, "Локальный git-репозиторий уже существует.");
    }

    // Если имя/почта не заданы глобально — задаём локально, иначе commit не пройдёт.
    if git(&dir, &["config", "user.name"]).is_err() {
        let _ = git(&dir, &["config", "user.name", "ADB Wizard"]);
    }
    if git(&dir, &["config", "user.email"]).is_err() {
        let _ = git(&dir, &["config", "user.email", "adb-wizard@users.noreply.github.com"]);
    }

    send(tx, "git add -A…");
    git(&dir, &["add", "-A"])?;

    let status = git(&dir, &["status", "--porcelain"])?;
    if status.trim().is_empty() {
        send(tx, "Нет изменений для коммита.");
    } else {
        send(tx, "git commit…");
        git(&dir, &["commit", "-m", "ADB Wizard: initial commit"])?;
        send(tx, "Коммит создан.");
    }

    // 2. gh
    let gh_version = match run("gh", &["--version"], &dir) {
        Ok(v) => v,
        Err(_) => {
            send(
                tx,
                "GitHub CLI (gh) не найден. Локальный репозиторий уже готов к публикации.",
            );
            send(tx, "Скачайте gh: https://cli.github.com/");
            send(tx, "После установки выполните в терминале: gh auth login");
            send(tx, "Затем нажмите «Опубликовать на GitHub» ещё раз.");
            return Err(
                "gh не установлен — локальный git готов, но автоматическая публикация недоступна."
                    .into(),
            );
        }
    };
    send(tx, format!("Найден {}", gh_version.lines().next().unwrap_or("gh")));

    if gh(&dir, &["auth", "status"]).is_err() {
        send(tx, "gh не авторизован.");
        send(tx, "Выполните в терминале: gh auth login");
        return Err("Нужна авторизация GitHub CLI (gh auth login).".into());
    }

    // 3. Создание репозитория и push
    let visibility = if private { "--private" } else { "--public" };
    let mut args = vec![
        "repo",
        "create",
        repo,
        visibility,
        "--source",
        ".",
        "--remote",
        "origin",
        "--push",
    ];
    if !description.trim().is_empty() {
        args.push("--description");
        args.push(description.trim());
    }

    let has_origin = git(&dir, &["remote", "get-url", "origin"]).is_ok();
    if has_origin {
        send(tx, "Remote origin уже настроен — делаю push…");
        let branch = git(&dir, &["branch", "--show-current"])?;
        let branch = if branch.trim().is_empty() {
            "main".to_string()
        } else {
            branch.trim().to_string()
        };
        git(&dir, &["push", "-u", "origin", &branch])?;
    } else {
        send(tx, format!("gh repo create {repo} {visibility} --push…"));
        if let Err(e) = gh(&dir, &args) {
            let lower = e.to_lowercase();
            if lower.contains("already exists") || lower.contains("already_exists") {
                send(tx, "Репозиторий уже существует — настраиваю remote и пушу.");
                let login = gh(&dir, &["api", "user", "--jq", ".login"])?
                    .trim()
                    .to_string();
                let url = format!("https://github.com/{login}/{repo}.git");
                if has_origin {
                    let _ = git(&dir, &["remote", "set-url", "origin", &url]);
                } else {
                    git(&dir, &["remote", "add", "origin", &url])?;
                }
                let branch = git(&dir, &["branch", "--show-current"])?;
                let branch = if branch.trim().is_empty() {
                    "main".to_string()
                } else {
                    branch.trim().to_string()
                };
                git(&dir, &["push", "-u", "origin", &branch])?;
            } else {
                return Err(format!("gh repo create: {e}"));
            }
        }
    }

    let url = gh(&dir, &["repo", "view", repo, "--json", "url", "--jq", ".url"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("https://github.com/{repo}"));

    send(tx, format!("Готово: {url}"));
    Ok(url)
}
