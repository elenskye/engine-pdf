# 0002 App 用 GitHub 仓库 + tag 引用固定版本

2026-11-24 站主定（ASK）。

- 仓库：`github.com/elenskye/engine-pdf`（私有可以）。站主建远端、push。
- 每次发版 Claude 打 tag `v<版本>`（如 `v0.4.0`），push 由站主做（`git push --tags`）。发出去的 tag 永远不改、不删。
- App 的 `Cargo.toml`：

  ```toml
  engine-pdf = { git = "https://github.com/elenskye/engine-pdf", tag = "v0.4.0" }
  ```

  仓库是私有的话，App 根目录 `.cargo/config.toml` 加：

  ```toml
  [net]
  git-fetch-with-cli = true
  ```

  （让 cargo 用系统 git 和钥匙串里的 GitHub 凭据拉取。）
- 升级 = 改 tag；回退 = 改回上一个 tag。`Cargo.lock` 记着确切的 commit，各 App 独立升级、回退，互不影响。
- 引擎静态链进 App 的二进制，随 App 的安装包发布，不另装、不起进程。

没选的：本机 `file://` 路径（绑死这台 Mac）、复制进各 App 的 `vendor/`（每次升级要同步一次）。

2026-11-25 补：仓库实际是公开的，App 不需要 `git-fetch-with-cli` 那段配置。
