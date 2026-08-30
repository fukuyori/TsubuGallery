# 版上げの手順

版番号は 1 か所だけ。Cargo.lock は `cargo build` が書き換える。インストーラの
スクリプト (`scripts/build-installer.ps1` / `build-macos-installer.sh`) と
`--version` は Cargo.toml から読むので、手で触る場所はない。

| ファイル | 直すところ |
|---|---|
| `Cargo.toml` | `[workspace.package]` の `version` |
| `CHANGELOG.md` | `## Unreleased` を `## X.Y.Z — YYYY-MM-DD` に |
| `CHANGELOG.ja.md` | `## 未リリース` を同じく |
| `Cargo.lock` | `cargo build` (または `cargo test`) で自動更新。コミットに含める |

## 確認

```powershell
cargo test            # PowerShell から (Git Bash 経由では ConPTY テストがハングする)
git grep -n "<旧版>"  # 取りこぼしが無いか
```

インストーラの作成は証明書で署名できる環境だけで行う。版上げの作業自体は
ここまで (コミット・タグ・プッシュは指示があったときだけ)。
