# 发货办理页视觉验收

final result: blocked

## 验收基准

- 日期：2026-11-06。范围：收货地址索引、物流录入、商品表与拆分包裹。
- source visual truth path: `/Users/huangjiajiang/.codex/generated_images/01a1b054-f787-70b3-bf0c-4da301be5238/exec-199d3608-4f3e-45d6-a1eb-053f91b9cb77.png`，用户选定的第 1 张图。
- source pixel dimensions: 1441 × 1092。
- implementation screenshot path: 未采集。
- viewport / implementation pixel dimensions / CSS size / density normalization: 未采集；验收时须统一视口和像素密度。
- state: 浅色主题、供应商直发、多个收货地址；须覆盖单包裹、多包裹、100 地址列表和 100 商品列表。

## 验收约束

遵守 `erp-client/AGENTS.md`：未获用户明确要求，不使用浏览器或电脑操作技能。不得以编译结果代替视觉或真实发货验收。

full-view comparison evidence: 无实现截图，未对照。
focused region comparison evidence: 地址索引、物流行、商品表均须补充截图对照。
comparison history: 未进行视觉对照，不标记视觉通过。

## 执行检查

| 项目 | 验收要求 | 当前证据 |
| --- | --- | --- |
| 字体与文案 | 单行地址索引；移除步骤标题与默认说明段落 | 源码已调整，视觉待验收 |
| 间距与布局 | 物流横排位于商品表上方；地址、商品列表各自滚动 | 源码已调整，视觉待验收 |
| 颜色与图标 | 复用主题令牌、Lucide 图标，选中地址与完成状态可辨识 | 主题检查通过，视觉待验收 |
| 内容与数据 | 使用真实收货信息；不得把设计图示例写入业务数据 | 静态复核通过 |
| 主操作 | 数量输入、拆包、保存草稿、校验定位保持可用 | 原有 6 个输入处理回调静态比对一致；运行态待验收 |
| 导航与筛选 | 下一未完成跳过已填和不发货地址；搜索保留录入 | 源码已调整，点击验收待完成 |
| 控制台 | 无新增运行错误 | 未检查 |

Lint、TypeScript、构建、改动文件格式和 diff 检查通过。全项目格式检查仍报告 5 个未修改的财务模块文件，不纳入本次修复范围。

## 后续验收要求

1. 在授权使用浏览器后，采集同视口、同状态的实现截图，与第 1 张设计图并排检查。
2. 验证 100 地址切换、100 商品搜索与滚动、数量修改、拆包取消及确认、错误定位和草稿保留。
3. 修复实际发现的差异，重新截图复核；证据齐全后方可将本节 final result 改为 passed。

---

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
