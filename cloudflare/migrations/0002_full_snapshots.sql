CREATE TABLE full_contributions (
    contributor TEXT PRIMARY KEY,
    revision INTEGER NOT NULL CHECK (revision >= 1),
    cells TEXT NOT NULL CHECK (json_valid(cells)),
    updated_at TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE replaced_contributors (
    legacy_contributor TEXT PRIMARY KEY,
    contributor TEXT NOT NULL REFERENCES full_contributions(contributor)
) WITHOUT ROWID;

-- Preserve finalized data whose original weekly uploads already expired.
CREATE TABLE legacy_archive AS
SELECT season, updated_at, entries FROM snapshots
WHERE NOT EXISTS (SELECT 1 FROM contributions WHERE contributions.season = snapshots.season);
