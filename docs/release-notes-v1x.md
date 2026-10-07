# LogLens v1.x 发布说明（草稿 · 未发布）

- @author 十四叔
- @date 2026/10/07
- 状态: **草稿** —— 发布日前定稿；购买链接 / 版本号 / 日期均为占位
- 用法: GitHub Release 正文 = 本文件去掉头部元信息段；README 购买节与商店
  文案的早鸟段落从 §二 取；**早鸟到期/到量后删 §二**（会过期的文字不留常驻文案）

## TL;DR

LogLens v1.x adds the paid tier: multi-file timeline merge, field analytics,
export, and named workspace sessions — perpetual license, $29 personal /
$59 business per seat. **Launch early bird: $19 / $39** (first month or first
50 orders, whichever comes first). The free tier stays free forever, unchanged.

## 一、v1.x 新在哪（付费层四腿 + 免费层三连）

付费层（买断制，一次付费含 v1.x 全部更新，换机自由）：

- **多文件时间线合并**：最多 8 源按时间戳归并；req_id 一键跨源追踪；
  时钟偏移校准、按源着色/隐藏、合并源组存进会话。3×1 GiB 混合源合并就绪
  **实测 947–967 ms** —— 比 LogViewPlus 单开 1 GB（本机实测 60 s）还快约 40 倍
- **字段分析**：任意 JSON 字段的数值分位数（p50/p99）与枚举分布，
  跟随过滤行集（1 GiB 单列实测 1001 ms）
- **导出**：过滤结果出原始行 / JSON 美化 / Excel 就绪 CSV
  （1 GiB raw 实测 608 ms；可取消，不留半成品）
- **命名工作台会话**：过滤 / 搜索 / 摆列 / 展开 / 合并源组，一键存取排障现场

免费层同期新增（**继续永久免费**）：列宽拖拽 / 显隐 / 重排、书签按文件持久化、
免语法字段点选查询、Warn 浮层提示分级、复选框、主题与更新提示打磨。

## 二、首发早鸟（首月或前 50 单，先到为准）

- Personal：**$29 → $19**；Business：**$59/seat → $39/seat**
- checkout 页自动显示划线原价 + 折后价，**无需优惠码**
- 到期/到量自动回正价；正价也不是「涨价」——$29/$59 就是长期标价

## 三、怎么买

- **GitHub 轨（Paddle 代收）**：购买链接 → [占位：发布日回填 Paddle checkout link]
  付款后 license key 经**邮件交付**（通常 24 小时内）；设置 → 许可 → 粘贴激活，
  离线校验零网络请求。**30 天无理由退款**（EULA §5）
- **Microsoft Store 轨**：add-on「完整版」店内购买（上架时间以后续公告为准）
- **免费层**：GitHub Releases 与商店继续免费，功能不变

## 四、合规一句话

源代码 MIT OR Apache-2.0；官方二进制与 license key 适用
[EULA](eula.md)——免费层限个人/非商业；个人自购 $29 可商用；
组织出资/报销 $59/seat。
