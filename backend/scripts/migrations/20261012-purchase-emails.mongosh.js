// 执行：mongosh <目标数据库连接> --file 本文件。
// 前置：备份目标库；只登记新集合索引，不回填销售历史地址，不修改角色。
// 回滚：停用发件入口并保留 purchase_emails 与唯一索引，禁止删记录后重发。
db.purchase_emails.createIndex({ id: 1 }, { name: "uk_purchase_email_id", unique: true });
db.purchase_emails.createIndex({ purchase_order_id: 1, created_at: -1, id: -1 }, { name: "ix_purchase_email_order" });
print("purchase_emails 唯一索引已就绪；历史销售 delivery_allocations 缺省按空列表读取。");
