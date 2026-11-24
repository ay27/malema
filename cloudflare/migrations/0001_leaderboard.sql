CREATE TABLE contributions (
    season INTEGER NOT NULL,
    week INTEGER NOT NULL,
    contributor TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision >= 1),
    cells TEXT NOT NULL CHECK (json_valid(cells)),
    PRIMARY KEY (season, week, contributor)
) WITHOUT ROWID;

CREATE TABLE snapshots (
    season INTEGER PRIMARY KEY,
    updated_at TEXT NOT NULL,
    entries TEXT NOT NULL CHECK (json_valid(entries))
);
