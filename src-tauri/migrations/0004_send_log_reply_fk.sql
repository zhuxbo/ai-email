-- send_log.in_reply_to 原缺 ON DELETE 子句（默认 RESTRICT）：删除被回复过的邮件会触发
-- FOREIGN KEY constraint failed。改为 ON DELETE SET NULL —— 保留审计行、置空引用。
-- SQLite 不支持 ALTER 列的外键，按官方「重建表」流程迁移；send_log 无被引用，事务内安全。
CREATE TABLE send_log_new (
    id              BLOB    PRIMARY KEY,
    account_id      BLOB    NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    in_reply_to     BLOB    REFERENCES messages(id) ON DELETE SET NULL,
    to_addrs        TEXT    NOT NULL DEFAULT '[]',
    subject         TEXT    NOT NULL,
    ai_assisted     INTEGER NOT NULL,
    sent_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
    smtp_response   TEXT
);
INSERT INTO send_log_new SELECT * FROM send_log;
DROP TABLE send_log;
ALTER TABLE send_log_new RENAME TO send_log;
