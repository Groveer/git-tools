use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;
use tracing_subscriber::prelude::*;

mod ai;
mod config;
mod git;

use config::Settings;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// The Git repository path
    #[arg(short, long, default_value = ".")]
    repo: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 合并分支并使用AI解决冲突
    Merge {
        /// The target branch to merge into
        #[arg(short, long)]
        target: String,

        /// The source branch to merge from
        #[arg(short, long)]
        source: String,
    },
    /// 列出目标分支中不在源分支中的提交
    ListUnique {
        /// The target branch to examine
        #[arg(short, long)]
        target: String,

        /// The source branch to compare against
        #[arg(short, long)]
        source: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化 journald 日志订阅器
    let journald_layer = tracing_journald::layer().expect("Failed to connect to systemd-journald");

    // 设置全局日志订阅器
    tracing_subscriber::registry().with(journald_layer).init();

    // Parse command line arguments
    let cli = Cli::parse();

    // Create GitHandler instance
    let git = git::GitHandler::new(&cli.repo)?;

    match &cli.command {
        Command::Merge { target, source } => {
            // 只在需要使用AI时加载配置
            let config = match Settings::load() {
                Ok(config) => config,
                Err(err) => {
                    eprintln!("警告: 无法加载OpenAI配置: {}", err);
                    eprintln!("将在没有AI辅助的情况下继续执行合并，如有冲突需手动解决");
                    Settings::default()
                }
            };

            handle_merge(&git, target, source, config).await
        }
        Command::ListUnique { target, source } => handle_list_unique(&git, target, source),
    }
}

async fn handle_merge(
    git: &git::GitHandler,
    target: &str,
    source: &str,
    config: Settings,
) -> Result<()> {
    // Verify branches exist
    if !git.branch_exists(target)? {
        return Err(anyhow::anyhow!("Target branch '{}' does not exist", target));
    }
    if !git.branch_exists(source)? {
        return Err(anyhow::anyhow!("Source branch '{}' does not exist", source));
    }

    // Attempt to merge
    let has_conflicts = git.merge_branches(target, source)?;

    if has_conflicts {
        println!("合并产生冲突。正在获取冲突详情...");
        let conflicts = git.get_conflicts()?;

        for conflict in &conflicts {
            println!("\n文件冲突: {}", &conflict.path);
            println!("我们的版本:\n{}", &conflict.our_content);
            println!("他们的版本:\n{}", &conflict.their_content);
            if let Some(base) = &conflict.base_content {
                println!("基础版本:\n{}", base);
            }
        }

        // 检查是否有有效的API密钥来使用AI解决冲突
        if config.openai_api_key.is_some() {
            println!("\n正在尝试使用AI解决冲突...");

            // Create AI conflict resolver
            let resolver = ai::ConflictResolver::new(config);

            let mut all_resolved = true;
            for conflict in &conflicts {
                println!("\n解决文件冲突: {}", conflict.path);
                match resolver.resolve_conflict(conflict).await {
                    Ok(resolution) => {
                        println!("AI建议的解决方案:\n{}", resolution);
                        match git.apply_resolution(&conflict.path, &resolution) {
                            Ok(_) => println!("✓ 解决方案应用成功"),
                            Err(e) => {
                                println!("✗ 应用解决方案失败: {}", e);
                                all_resolved = false;
                            }
                        }
                    }
                    Err(e) => {
                        println!("✗ 获取AI解决方案失败: {}", e);
                        all_resolved = false;
                    }
                }
            }

            if all_resolved {
                println!("\n所有冲突已成功解决！");
                println!("请检查更改并提交。");
            } else {
                git.abort_merge()?;
                println!("\n某些冲突无法自动解决。");
                println!("合并已中止。请手动解决剩余冲突。");
            }
        } else {
            git.abort_merge()?;
            println!("\n未配置OpenAI API密钥，无法使用AI解决冲突。");
            println!("合并已中止。请手动解决冲突，或配置API密钥后重试。");
        }
    } else {
        println!("合并成功完成！");
    }

    Ok(())
}

fn handle_list_unique(git: &git::GitHandler, target: &str, source: &str) -> Result<()> {
    // 验证分支是否存在
    if !git.branch_exists(target)? {
        return Err(anyhow::anyhow!("目标分支 '{}' 不存在", target));
    }
    if !git.branch_exists(source)? {
        return Err(anyhow::anyhow!("源分支 '{}' 不存在", source));
    }

    // 获取不在源分支中的目标分支提交
    println!("列出 '{}' 中不在 '{}' 中的提交:", target, source);
    let unique_commits = git.list_unique_commits(target, source)?;

    if unique_commits.is_empty() {
        println!("没有发现独有的提交。");
    } else {
        println!("发现 {} 个独有的提交:", unique_commits.len());
        for (i, (commit_id, message)) in unique_commits.iter().enumerate() {
            // 分割提交信息，获取标题和详细内容
            let parts: Vec<&str> = message.splitn(2, '\n').collect();
            let title = parts[0];
            let details = if parts.len() > 1 { parts[1] } else { "" };

            // 使用不同颜色高亮显示序号、哈希、标题，内容使用暗淡颜色
            println!(
                "{}. {} - {}{}",
                (i + 1).to_string().cyan().bold(), // 序号使用青色加粗
                commit_id.to_string()[..7].yellow(), // 哈希值前7位使用黄色
                title.green().bold(),              // 标题使用绿色加粗
                if !details.is_empty() {
                    format!("\n   {}", details.dimmed()) // 内容使用暗淡显示，并缩进
                } else {
                    String::new()
                }
            );
        }
    }

    Ok(())
}
