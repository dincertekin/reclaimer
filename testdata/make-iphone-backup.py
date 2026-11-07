# Builds testdata/iphone-backups/ — two fake iPhone backups laid out exactly
# like Finder/iTunes makes them:
#   00008110-000A1B2C3D4E5F6A/   an ordinary backup:
#       Camera Roll: IMG_0001.JPG, and IMG_0002.JPG which Photos.sqlite marks
#                    as in Recently Deleted
#       Messages:    Menu.pdf (an attachment)
#       WhatsApp:    one image
#       plus a settings file that is not a photo or attachment
#   00008110-000F00DFEEDBEEF01/  an encrypted backup (listed, not readable)
#   python3 testdata/make-iphone-backup.py
import datetime, hashlib, os, plistlib, shutil, sqlite3

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "iphone-backups")
shutil.rmtree(ROOT, ignore_errors=True)

image = open(os.path.join(HERE, "..", "test.img"), "rb").read()
photo1, photo2 = image[1024:1024 + 94978], image[102400:102400 + 42355]
pdf = b"%PDF-1.4\n1 0 obj <<>> endobj\ntrailer <<>>\n%%EOF\n"
modified = 1793990100  # 6 Nov 2026 18:35 UTC

def info(udid, encrypted):
    d = os.path.join(ROOT, udid)
    os.makedirs(d)
    plistlib.dump({
        "Device Name": "Test iPhone",
        "Display Name": "Test iPhone",
        "Product Name": "iPhone 15",
        "Product Type": "iPhone15,4",
        "Unique Identifier": udid,
        "Last Backup Date": datetime.datetime(2026, 11, 3, 20, 15, tzinfo=datetime.timezone.utc),
    }, open(os.path.join(d, "Info.plist"), "wb"))
    plistlib.dump({"IsEncrypted": encrypted, "Version": "10.0"}, open(os.path.join(d, "Manifest.plist"), "wb"))
    return d

def file_blob(path, size):
    # What iOS stores per file: an NSKeyedArchiver archive of an MBFile.
    return plistlib.dumps({
        "$version": 100000, "$archiver": "NSKeyedArchiver", "$top": {"root": plistlib.UID(1)},
        "$objects": ["$null",
                     {"$class": plistlib.UID(3), "RelativePath": plistlib.UID(2), "Size": size,
                      "LastModified": modified, "Birth": modified, "Mode": 0o100644},
                     path,
                     {"$classname": "MBFile", "$classes": ["MBFile", "NSObject"]}],
    }, fmt=plistlib.FMT_BINARY)

# --- the ordinary backup -------------------------------------------------
d = info("00008110-000A1B2C3D4E5F6A", False)

photos_db = os.path.join(HERE, "photos.tmp")
if os.path.exists(photos_db):
    os.remove(photos_db)
p = sqlite3.connect(photos_db)
p.execute("CREATE TABLE ZASSET (Z_PK INTEGER PRIMARY KEY, Z_ENT INTEGER, ZTRASHEDSTATE INTEGER, ZDIRECTORY VARCHAR, ZFILENAME VARCHAR)")
p.execute("INSERT INTO ZASSET VALUES (1, 3, 0, 'DCIM/100APPLE', 'IMG_0001.JPG')")
p.execute("INSERT INTO ZASSET VALUES (2, 3, 1, 'DCIM/100APPLE', 'IMG_0002.JPG')")
p.commit(); p.close()
photos = open(photos_db, "rb").read()
os.remove(photos_db)

entries = [
    ("CameraRollDomain", "Media/DCIM", 2, None),
    ("CameraRollDomain", "Media/DCIM/100APPLE", 2, None),
    ("CameraRollDomain", "Media/DCIM/100APPLE/IMG_0001.JPG", 1, photo1),
    ("CameraRollDomain", "Media/DCIM/100APPLE/IMG_0002.JPG", 1, photo2),
    ("CameraRollDomain", "Media/PhotoData/Photos.sqlite", 1, photos),
    ("MediaDomain", "Library/SMS/Attachments/3a/10/7C1D-44E2/Menu.pdf", 1, pdf),
    ("AppDomainGroup-group.net.whatsapp.WhatsApp.shared",
     "Message/Media/447700900123@s.whatsapp.net/9/f/9f8e2d1c.jpg", 1, photo2),
    ("HomeDomain", "Library/Preferences/com.apple.example.plist", 1, b"<plist/>"),
]
m = sqlite3.connect(os.path.join(d, "Manifest.db"))
m.execute("CREATE TABLE Files (fileID TEXT PRIMARY KEY, domain TEXT, relativePath TEXT, flags INTEGER, file BLOB)")
m.execute("CREATE TABLE Properties (key TEXT PRIMARY KEY, value BLOB)")
for domain, path, flags, data in entries:
    file_id = hashlib.sha1(f"{domain}-{path}".encode()).hexdigest()
    m.execute("INSERT INTO Files VALUES (?, ?, ?, ?, ?)",
              (file_id, domain, path, flags, file_blob(path, len(data) if data else 0)))
    if data is not None:
        os.makedirs(os.path.join(d, file_id[:2]), exist_ok=True)
        open(os.path.join(d, file_id[:2], file_id), "wb").write(data)
m.commit(); m.close()

# --- the encrypted backup -------------------------------------------------
d = info("00008110-000F00DFEEDBEEF01", True)
open(os.path.join(d, "Manifest.db"), "wb").write(os.urandom(4096))
print("made", ROOT)
