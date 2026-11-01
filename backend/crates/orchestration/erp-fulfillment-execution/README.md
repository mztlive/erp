# 履约执行编排

本库持有履约、库存调整与供应执行命令的跨域事务。领域事实继续由拥有领域仓储和服务写入；所有事务步骤沿用入口提供的同一 `Executor`。

应用必须注入授权、工作项、库存和外部供应商能力。外部调用只允许发生在数据库事务之外；未知提交仅按原命令身份查证。

库测试执行 `env -u ERP_TEST_MONGO_URI cargo test -p erp-fulfillment-execution --lib --locked`。禁止执行集成测试、ignored 测试或真实服务验收。
