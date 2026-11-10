# Regenerates DataCache.db, a synthetic stand-in for Stream Dock's config\DataCache.db (no real data).
import os, sqlite3
p = os.path.join(os.path.dirname(__file__), "DataCache.db")
if os.path.exists(p): os.remove(p)
c = sqlite3.connect(p)
c.execute("CREATE TABLE AJAZZ (Device TEXT, serialNumber TEXT)")
c.execute("INSERT INTO AJAZZ VALUES ('AJAZZ AKP153E', NULL)")
c.execute("CREATE TABLE AKP153E (Profile TEXT)")
for s in ["3I0U64IN-IT4R-RUB6-EZ4Z-VXS311JFXI05", "R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88", "H33G4VTZ-25Z8-PYN7-R34M-ST39SX8TRT2Q"]:
    c.execute("INSERT INTO AKP153E VALUES (?)", (s + ".sdProfile",))
c.execute("CREATE TABLE UnknownDevice (Profile TEXT)")
c.execute("CREATE TABLE Big (Name TEXT, Pad TEXT)")
c.executemany("INSERT INTO Big VALUES (?, ?)", [("row%d" % i, "x" * 40) for i in range(600)])
c.commit(); c.close()
