# Builds testdata/sqlite-test.db with the real SQLite, for the reader's tests:
# small pages (so the table is a multi-level tree), a blob bigger than a page
# (so it spills onto overflow pages), every value type, and a column added
# later with ALTER TABLE (so older rows are shorter than the schema).
#   python3 testdata/make-sqlite.py
import os, sqlite3
path = os.path.join(os.path.dirname(__file__), "sqlite-test.db")
if os.path.exists(path):
    os.remove(path)
db = sqlite3.connect(path)
db.execute("PRAGMA page_size = 1024")
db.execute("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, size INTEGER, ratio REAL, data BLOB)")
for i in range(1, 2001):
    db.execute("INSERT INTO items VALUES (?, ?, ?, ?, ?)",
               (i, f"item {i}", i * 1000 - 500, i / 4, bytes([i % 256]) * (i % 7)))
db.execute("UPDATE items SET data = ? WHERE id = 1000", (bytes(range(256)) * 20,))  # 5120 bytes
db.execute("UPDATE items SET size = NULL, ratio = -1.5, name = 'naïve ✓' WHERE id = 7")
db.execute("ALTER TABLE items ADD COLUMN note TEXT")
db.execute("UPDATE items SET note = 'later' WHERE id = 2000")
db.commit()
db.execute("VACUUM")
db.close()
print("made", path, os.path.getsize(path), "bytes")
