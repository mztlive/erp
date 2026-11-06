// 银行联行号发布检查。无需回填：缺省字段按 null 读取，不得编造历史联行号。
// 执行：mongosh <connection> --file 20261106-bank-routing-number.mongosh.js
// 本脚本只检查已存在字段的格式，不写入数据，不输出账户资料。
// 回滚：回退应用版本并保留可选字段；旧版本忽略该字段。不得 unset 已维护资料。
const invalidRoutingNumbers = db.party_bank_accounts.countDocuments({
    bank_routing_number: { $exists: true, $ne: null, $not: /^\d{12}$/ },
});
if (invalidRoutingNumbers > 0) {
    throw new Error(`发现 ${invalidRoutingNumbers} 条联行号格式异常，须在发布前修复`);
}
print("银行联行号检查通过；无需回填历史账户");
