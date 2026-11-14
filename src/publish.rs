//! Публикация проекта на GitHub через git + GitHub CLI (gh).

use crate::i18n::{tr, Lang};
use std::ffi::OsStr;
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

/// Ищет gh в PATH, рядом с приложением и в стандартных папках установки.
fn gh_program() -> PathBuf {
    if let Ok(p) = std::env::var("GH_PATH") {
        let p = PathBuf::from(p);
        if p.exists() {
            return p;
        }
    }

    let local = std::env::var("LOCALAPPDATA").ok();
    let program_files = std::env::var("ProgramFiles").ok();

    let mut candidates = Vec::new();
    if let Some(local) = local {
        let base = PathBuf::from(local).join("Programs").join("GitHub CLI");
        candidates.push(base.join("bin").join("gh.exe"));
        candidates.push(base.join("gh.exe"));
    }
    if let Some(pf) = program_files {
        candidates.push(PathBuf::from(pf).join("GitHub CLI").join("gh.exe"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("gh.exe"));
        }
    }

    for c in candidates {
        if c.exists() {
            return c;
        }
    }
    PathBuf::from("gh")
}

fn run_path(
    program: impl AsRef<OsStr>,
    args: &[&str],
    dir: &Path,
    token: Option<&str>,
) -> Result<String, String> {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.args(args).current_dir(dir);
    if let Some(token) = token {
        if !token.trim().is_empty() {
            cmd.env("GH_TOKEN", token.trim());
        }
    }

    let out = cmd
        .output()
        .map_err(|e| format!("Не удалось запустить {}: {e}", cmd.get_program().to_string_lossy()))?;

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

fn run(program: &str, args: &[&str], dir: &Path) -> Result<String, String> {
    run_path(program, args, dir, None)
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    run("git", args, dir)
}

fn gh(dir: &Path, args: &[&str], token: &str) -> Result<String, String> {
    run_path(gh_program(), args, dir, Some(token))
}

pub fn tools() -> Tools {
    let dir = project_dir();
    Tools {
        git: run("git", &["--version"], &dir)
            .ok()
            .map(|s| s.trim().to_string()),
        gh: run_path(gh_program(), &["--version"], &dir, None)
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

pub fn publish(
    repo: &str,
    description: &str,
    private: bool,
    token: &str,
    lang: Lang,
    tx: &Sender<PublishEvent>,
) {
    let result = publish_inner(repo, description, private, token, lang, tx);
    let _ = tx.send(PublishEvent::Done(result));
}

fn publish_inner(
    repo: &str,
    description: &str,
    private: bool,
    token: &str,
    lang: Lang,
    tx: &Sender<PublishEvent>,
) -> Result<String, String> {
    let dir = project_dir();
    send(
        tx,
        format!(
            "{} {}",
            tr(lang, "Папка проекта:", "Project folder:"),
            dir.display()
        ),
    );

    if !valid_repo_name(repo) {
        return Err(tr(
            lang,
            "Имя репозитория может содержать только латиницу, цифры, '-', '_' и '.'",
            "Repository name may contain only ASCII letters, digits, '-', '_' and '.'",
        )
        .into());
    }

    // 1. git
    if run("git", &["--version"], &dir).is_err() {
        return Err(tr(
            lang,
            "git не найден. Установите Git for Windows: https://git-scm.com/download/win",
            "git not found. Install Git: https://git-scm.com/downloads",
        )
        .into());
    }

    if !dir.join(".git").exists() {
        send(tx, "git init…");
        git(&dir, &["init", "-b", "main"])
            .or_else(|_| git(&dir, &["init"]).and_then(|_| git(&dir, &["checkout", "-b", "main"])))?;
        send(
            tx,
            tr(
                lang,
                "Локальный git-репозиторий создан (ветка main).",
                "Local git repository created (branch main).",
            ),
        );
    } else {
        send(
            tx,
            tr(
                lang,
                "Локальный git-репозиторий уже существует.",
                "Local git repository already exists.",
            ),
        );
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
        send(
            tx,
            tr(lang, "Нет изменений для коммита.", "No changes to commit."),
        );
    } else {
        send(tx, "git commit…");
        git(&dir, &["commit", "-m", "ADB Wizard: initial commit"])?;
        send(tx, tr(lang, "Коммит создан.", "Commit created."));
    }

    // 2. gh
    let gh_version = match run_path(gh_program(), &["--version"], &dir, None) {
        Ok(v) => v,
        Err(_) => {
            send(
                tx,
                tr(
                    lang,
                    "GitHub CLI (gh) не найден. Локальный репозиторий уже готов к публикации.",
                    "GitHub CLI (gh) not found. The local repository is ready to publish.",
                ),
            );
            send(tx, tr(lang, "Скачайте gh: https://cli.github.com/", "Download gh: https://cli.github.com/"));
            send(
                tx,
                tr(
                    lang,
                    "После установки выполните в терминале: gh auth login",
                    "After installing, run in a terminal: gh auth login",
                ),
            );
            send(
                tx,
                tr(
                    lang,
                    "Затем нажмите «Опубликовать на GitHub» ещё раз.",
                    "Then press “Publish to GitHub” again.",
                ),
            );
            return Err(tr(
                lang,
                "gh не установлен — локальный git готов, но автоматическая публикация недоступна.",
                "gh is not installed — the local git repo is ready, but automatic publishing is unavailable.",
            )
            .into());
        }
    };
    send(
        tx,
        format!(
            "{} {}",
            tr(lang, "Найден", "Found"),
            gh_version.lines().next().unwrap_or("gh")
        ),
    );

    let has_token = !token.trim().is_empty();
    if has_token {
        send(
            tx,
            tr(
                lang,
                "Использую токен из поля (GH_TOKEN), авторизация не требуется.",
                "Using the token from the field (GH_TOKEN); no login required.",
            ),
        );
    } else if gh(&dir, &["auth", "status"], "").is_err() {
        send(tx, tr(lang, "gh не авторизован.", "gh is not authenticated."));
        send(
            tx,
            tr(
                lang,
                "Выполните в терминале: gh auth login",
                "Run in a terminal: gh auth login",
            ),
        );
        send(
            tx,
            tr(
                lang,
                "Либо вставьте Personal Access Token в поле «Токен» (права repo).",
                "Or paste a Personal Access Token into the “Token” field (repo scope).",
            ),
        );
        return Err(tr(
            lang,
            "Нужна авторизация GitHub: gh auth login или токен.",
            "GitHub authentication required: gh auth login or a token.",
        )
        .into());
    }

    let login = gh(&dir, &["api", "user", "--jq", ".login"], token)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if !login.is_empty() {
        send(
            tx,
            format!("{} {login}", tr(lang, "Аккаунт GitHub:", "GitHub account:")),
        );
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
        send(
            tx,
            tr(
                lang,
                "Remote origin уже настроен — делаю push…",
                "Remote origin is already configured — pushing…",
            ),
        );
        let branch = git(&dir, &["branch", "--show-current"])?;
        let branch = if branch.trim().is_empty() {
            "main".to_string()
        } else {
            branch.trim().to_string()
        };
        git(&dir, &["push", "-u", "origin", &branch])?;
    } else {
        send(tx, format!("gh repo create {repo} {visibility} --push…"));
        if let Err(e) = gh(&dir, &args, token) {
            let lower = e.to_lowercase();
            if lower.contains("already exists") || lower.contains("already_exists") {
                send(
                    tx,
                    tr(
                        lang,
                        "Репозиторий уже существует — настраиваю remote и пушу.",
                        "Repository already exists — configuring the remote and pushing.",
                    ),
                );
                let login = if login.is_empty() {
                    gh(&dir, &["api", "user", "--jq", ".login"], token)?
                        .trim()
                        .to_string()
                } else {
                    login.clone()
                };
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

    let url = gh(&dir, &["repo", "view", repo, "--json", "url", "--jq", ".url"], token)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if login.is_empty() {
                format!("https://github.com/{repo}")
            } else {
                format!("https://github.com/{login}/{repo}")
            }
        });

    send(tx, format!("{} {url}", tr(lang, "Готово:", "Done:")));
    Ok(url)
}
