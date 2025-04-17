# 🤖 Git-Tools AI版本Git助手

## 📝 项目简介

Git-Tools 是一个多功能的Git命令行辅助工具，集成了AI能力来简化常见的Git操作。它不仅可以在合并分支时自动解决冲突，还提供了分支分析功能，帮助用户更好地了解和管理代码库。当遇到复杂的分支管理问题时，Git-Tools能通过AI（使用自然语言模型的API）提供智能分析和建议，大幅提升开发效率。

## ✨ 特性

- 🔍 自动检测并展示Git合并冲突
- 🧠 使用AI（GPT模型）分析冲突内容
- 💡 智能提供冲突解决方案
- 🔄 自动应用AI建议的解决方案
- 📋 列出分支间独有的 commit 信息
- ⚙️ 支持配置自定义OpenAI API密钥和模型选择

## 🚀 安装

确保你已安装Rust和Cargo。然后克隆此仓库并编译：

```bash
git clone https://github.com/yourusername/git-tools.git
cd git-tools
cargo build --release
```

可执行文件将生成在`target/release/git-tools`。

## ⚙️ 配置

首次使用前，需要配置OpenAI API密钥。你可以通过以下方式之一进行配置：

1. 创建配置文件：

   ```bash
   cp config.json.example config.json
   ```

   然后编辑`config.json`文件，填入你的API密钥。

2. 或者设置环境变量（使用GT\_前缀）：
   ```bash
   export GT_OPENAI_API_KEY="your-api-key-here"
   export GT_MODEL="gpt-4"  # 可选，默认使用gpt-4
   export GT_MAX_RETRIES=3  # 可选，默认为3
   export GT_TIMEOUT_SECONDS=30  # 可选，默认为30秒
   ```

配置文件示例：

```json
{
  "openai_api_key": "your-api-key-here",
  "model": "gpt-4",
  "max_retries": 3,
  "timeout_seconds": 30
}
```

## 📋 使用方法

Git-Tools 提供了多个子命令来完成不同的任务：

### 合并分支并自动解决冲突

```bash
git-tools merge -t 目标分支 -s 源分支
```

例如，你希望将`feature`分支合并到`main`分支：

```bash
git-tools merge -t main -s feature
```

### 列出分支独有的 commit

查看一个分支中不存在于另一个分支的 commit：

```bash
git-tools list-unique -t 目标分支 -s 源分支
```

例如，查看 `feature` 分支中有哪些 `main` 分支中不存在的 commit：

```bash
git-tools list-unique -t feature -s main
```

### 完整参数说明

```
用法: git-tools [选项] <子命令>

选项:
  -r, --repo <REPO>      Git仓库路径 [默认: .]
  -h, --help             显示帮助信息
  -V, --version          显示版本信息

子命令:
  merge        合并分支并使用AI解决冲突
               参数:
               -t, --target <TARGET>  要合并到的目标分支
               -s, --source <SOURCE>  要从中合并的源分支

  list-unique  列出目标分支中不在源分支中的提交
               参数:
               -t, --target <TARGET>  要检查的目标分支
               -s, --source <SOURCE>  要比较的源分支

  help         显示此帮助信息或某个子命令的帮助信息
```

## 🔄 工作流程

### 合并分支

1. 🚦 工具会尝试将源分支合并到目标分支
2. ⚠️ 如果遇到冲突，会显示冲突详情
3. 🤖 对每个冲突文件，使用AI生成解决方案
4. 🔧 自动应用AI生成的解决方案
5. ✅ 如果所有冲突都成功解决，会提示用户检查并提交更改
6. ⚠️ 如果某些冲突无法自动解决，会中止合并并提示手动解决

### 列出独有 commit

1. 🔍 工具会检索目标分支中不存在于源分支的所有 commit
2. 📋 显示这些 commit 的哈希值和提交信息
3. 🧠 这有助于用户在合并前了解分支间的差异

## 👨‍💻 开发

项目结构：

- 📄 `src/main.rs` - 主程序入口
- 📄 `src/git.rs` - Git操作相关功能
- 📄 `src/ai.rs` - AI冲突解析实现
- 📄 `src/config.rs` - 配置管理

运行测试：

```bash
cargo test
```

## 📚 依赖库

- 📦 git2: Git操作
- 📦 reqwest: HTTP客户端
- 📦 tokio: 异步运行时
- 📦 serde: 序列化/反序列化
- 📦 clap: 命令行参数解析
- 📦 anyhow/thiserror: 错误处理
- 📦 tracing: 日志记录

## 📄 许可证

[待定]

## 🗺️ 未来计划(Roadmap)

我们计划为Git-Tools添加以下令人兴奋的功能：

- 🤖 AI提交代码 - 自动生成符合项目风格的提交信息
- 🔍 AI代码审核 - 自动检查并评审合并请求中的代码变更
- 🌐 多AI供应商支持 - 除OpenAI外，还将支持其他AI供应商(如Claude, Gemini等)
- 🔄 批量冲突解决 - 一次性解决多个文件中的所有冲突
- 🧩 插件系统 - 允许社区开发和共享自定义扩展
- 🔧 冲突解决策略 - 支持配置不同类型文件的解决策略

如果你对以上功能有任何建议或想法，请在Issues中告诉我们！

## 🤝 贡献

欢迎提交问题或拉取请求来帮助改进项目！

## ⚠️ 注意事项

- 🔑 需要有效的OpenAI API密钥才能使用AI功能
- 👀 建议在应用AI解决方案前进行代码审查
- 🛠️ 某些复杂冲突可能仍需人工干预
