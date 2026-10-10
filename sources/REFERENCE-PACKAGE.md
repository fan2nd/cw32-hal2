# CW32 原始参考资料交付范围

本资料集最初随 Stage28 补充包交付，随后合入源码包。当前 authored YAML 数量、版本及补充引文统一见 `SOURCES.md`；原始成员及其许可字节保持不变。

## 实际附带与未附带

- 附带 11 份官方 SDK 芯片头文件的原始字节；它们各自明确标注 Apache-2.0，保留原版权行，并附官方许可正文。它们是 SDK 内部成员，不是 11 个完整 SDK。
- 来源锁记载 45 个原始文件：31 PDF、12 SDK ZIP、2 官网 HTML，合计 273,867,065 字节。原始文件的 SHA-256、官方链接和实际版本仍以 项目的 `sources/evidence-sources.json` 为准，本资料集沿用这份锁，不新增重复 JSON。
- 本包没有附带上述 43 个完整 PDF/SDK。当前能确认的版权声明、示例使用/修改许可，没有明确覆盖整份手册或完整 SDK 的再分发。未明确授权不等于已发现禁止条款，也不等于可以宣称已获再分发权。
- 两份官网 HTML 快照也没有附带。产品目录另外引用的三个网页缓存也未附；它们不属于上述 45 项原件锁。官方锁定文件可由以下命令按原链接下载到本项目。
- `SOURCES.md` 给出 YAML 数据主题、原始文件版本、SDK 成员位置和已记录章节对应；不会用文件存在代替硬件事实核对。

## 获取原厂实物

在 `embassy-cw32/` 目录执行（Python 3，仅标准库）：

```sh
python3 cw32-data/tools/acquire_evidence.py --originals-only
```

默认获取 43 个硬件依据原件（31 PDF、12 SDK ZIP）到 `sources/vendor/`，按大小和 SHA-256 校验；字节变化会明确报错，不覆盖异文。两份 HTML 仅保留为历史发现记录，默认回执会明确列出这两项未获取，并标记 `complete_manifest=false`。此模式不需要 Poppler，不展开 SDK，也不生成 PDF 文本或中间报告。只取一个文件可追加 `--only 原文件名`；可重复指定。

验证已经下载的原件：

```sh
python3 cw32-data/tools/acquire_evidence.py --originals-only --verify
```

如需严格复验全部 45 项所选历史原件，在获取或验证命令后追加 `--include-discovery`；HTML 缺失或变化仍会失败。`./d refresh-discovery` 单独检查当前网页，将有界候选和回执写入忽略目录，不替换历史原件或更新锁。参见 [发现记录与硬件依据的范围](../docs/a030-discovery-policy.json)。

需要运行完整硬件事实审计时，再生成所需 SDK 成员和 PDF 文本（共 553 个硬件输入及元数据文件，需要项目记录的 Poppler 版本）：

```sh
python3 cw32-data/tools/acquire_evidence.py --offline
```

获取脚本不会执行 SDK 内代码。资料用于技术参考；第三方原件保留各自版权，项目许可证不覆盖它们。下载到本地和随本包交付是不同状态。

## 首次补充包（Stage28）记录的验证

- 对现存 45 个原始文件逐个校验大小与 SHA-256，全部通过。
- 在空目录从已校验缓存恢复一份 PDF 和一份 SDK，目录精确包含两份原件，没有派生文本/解压树/报告。
- 新增 `--originals-only` 的默认关闭，因此原有完整获取流程保持原路径。
- 附带头文件逐个对照原 SDK 成员哈希。构建通过和资料核对均不构成实板验证。

## 当前源码包检查

`ci/package-source.py` 要求资料目录精确包含 11 份已批准头文件和 README、SHA256SUMS、官方 Apache-2.0 许可。逐个校验原锁成员大小/哈希、原版权与 SPDX 声明，以及官方许可 SHA-256；现有禁止原件与历史内容的发行检查保持生效。`ci/run-verified.py` 将这些文件和资料索引一并纳入验证前后快照。完整 PDF、SDK、SVD、派生文本、生成 PAC 与运行日志仍不进入源码包。
