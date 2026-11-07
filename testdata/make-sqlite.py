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

# A database in WAL mode with changes still in its write-ahead log, as a live
# phone's Photos library often is: the main file has rows 1–2; the log marks
# row 2 trashed and adds row 3. Copied while open, before SQLite folds the
# log back into the main file.
import shutil
work = os.path.join(os.path.dirname(__file__), "wal-work.db")
for p in (work, work + "-wal", work + "-shm"):
    if os.path.exists(p):
        os.remove(p)
db = sqlite3.connect(work)
db.execute("PRAGMA journal_mode = WAL")
db.execute("PRAGMA wal_autocheckpoint = 0")
db.execute("CREATE TABLE ZASSET (Z_PK INTEGER PRIMARY KEY, ZTRASHEDSTATE INTEGER, ZFILENAME VARCHAR)")
db.execute("INSERT INTO ZASSET VALUES (1, 0, 'IMG_0001.HEIC'), (2, 0, 'IMG_0002.HEIC')")
db.commit()
db.execute("PRAGMA wal_checkpoint(TRUNCATE)")  # rows 1–2 now in the main file
db.execute("UPDATE ZASSET SET ZTRASHEDSTATE = 1 WHERE Z_PK = 2")
db.execute("INSERT INTO ZASSET VALUES (3, 0, 'IMG_0003.HEIC')")
db.commit()
out = os.path.join(os.path.dirname(__file__), "sqlite-wal.db")
shutil.copy(work, out)
shutil.copy(work + "-wal", out + "-wal")
db.close()
for p in (work, work + "-wal", work + "-shm"):
    if os.path.exists(p):
        os.remove(p)
print("made", out, "and its -wal")
