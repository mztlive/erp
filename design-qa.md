# 供应商 API 连接详情视觉验收

final result: blocked

## 验收对象

- 设计基准：用户选定的第 3 张设计图。
- source visual truth path: `/Users/huangjiajiang/.codex/generated_images/01a17785-fc7f-7693-af2e-c982cc217033/exec-3b2f40d9-72d0-4ba2-ae74-2173fe40749c.png`
- source pixel dimensions: 1487 × 1058。
- 页面：`/supplier-api/connections/ae7ab0f69d41421298718296edb8ab3a?section=health`。
- viewport：待采集，须覆盖桌面工作区与窄窗口。
- implementation screenshot path: 未采集。
- implementation pixel dimensions / CSS size / density normalization: 未采集，不得据此计算视觉偏差。
- state：健康记录分区；须分别验收未配置、未检查、验证通过、失败及结果待确认。

## 验收限制

执行本次任务时须遵守仓库“非我主动要求时，不要使用 use computer 或者 use browser 技能”的约束。不得以静态源码或构建结果代替实际页面验收。

full-view comparison evidence: 无实现截图，未对照。
focused region comparison evidence: 无实现截图，未对照。
comparison history: 无视觉对照轮次，不得标记为视觉通过。

## 验收项目

| 项目 | 通过条件 | 当前状态 |
| --- | --- | --- |
| 字体与排版 | 标题层级、正文行高、长名称换行与基准图一致 | 待浏览器验收 |
| 留白与布局 | 桌面双栏、窄窗口单栏；表格在容器内滚动 | 待浏览器验收 |
| 颜色与令牌 | 中性色页面、黑色主操作、语义状态色可辨识 | 主题静态检查通过，视觉待验收 |
| 图标与图像 | 图标清晰、尺寸一致；沿用项目图标库 | 待浏览器验收 |
| 内容与状态 | 检查结论和逐项能力验证分别消费真实数据，不得显示示例数据 | 静态复核完成，运行态待验收 |
| 主操作 | 健康检查、更多菜单及启停确认遵循服务端权限与阻断原因 | 待点击验收 |
| 分区与记录 | 建议按钮进入对应分区；列设置、分页和键盘导航可用 | 待点击验收 |
| 控制台 | 不得存在本次改动引入的运行错误 | 未检查 |

## 静态检查

- `npm run lint`：通过。
- `npx tsc --noEmit`：通过；最终生产构建内 TypeScript 检查通过。
- `npm run build`：通过。
- 修改范围 `oxfmt --check`：通过。
- `git diff --check`：通过。
- 全仓 `npm run format:check`：未通过；仅报告未修改的 `features/supplier-offerings/pages/supplier-offerings-page.tsx` 既有格式问题。

## 后续验收要求

1. 获得浏览器操作授权后，以相同视口采集设计基准和真实页面，完成全页及重点区域并排对照。
2. 验证主操作、分区跳转、列设置、列表与详情页签隔离、空态和权限受限状态。
3. 检查控制台错误并修复实际发现的布局或交互问题；完成重新截图与复核后方可将 final result 改为 passed。
