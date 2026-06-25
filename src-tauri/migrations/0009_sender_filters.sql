CREATE TABLE sender_filters (
    id         BLOB PRIMARY KEY,
    list_type  TEXT NOT NULL,
    match_type TEXT NOT NULL,
    pattern    TEXT NOT NULL,
    note       TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now'))
);

CREATE UNIQUE INDEX idx_sender_filters_unique
    ON sender_filters (list_type, match_type, pattern);
