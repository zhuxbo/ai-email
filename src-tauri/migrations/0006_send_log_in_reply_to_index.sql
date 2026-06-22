-- 0006_send_log_in_reply_to_index.sql (SQLite)
-- suggested_replies.list_pending 用 `message_id NOT IN (SELECT in_reply_to FROM send_log ...)`
-- 派生「已回复」排除；给 send_log.in_reply_to 建索引，避免随发件量增长全表扫该子查询。
CREATE INDEX idx_send_log_in_reply_to ON send_log (in_reply_to);
