// 执行目标：当前 ERP 数据库。新增通知集合索引，不修改任何既有业务记录。
// 回滚：保留集合及索引，恢复旧应用；禁止重置已 Delivered 的审批 outbox。
db.notifications.createIndex({ id: 1 }, { name: "uk_notifications_id", unique: true });
db.notifications.createIndex(
    { recipient_id: 1, dedup_key: 1 },
    { name: "uk_notifications_recipient_event", unique: true },
);
db.notifications.createIndex(
    { recipient_id: 1, created_at: -1, id: -1 },
    { name: "idx_notifications_inbox" },
);
db.notifications.createIndex(
    { recipient_id: 1, read_at: 1 },
    { name: "idx_notifications_unread" },
);
