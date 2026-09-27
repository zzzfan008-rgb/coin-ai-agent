#!/usr/bin/env python3
"""
批量为 fabrics 表生成 description_embedding
纯 stdlib: subprocess 调用 curl + psql
"""
import subprocess, json, sys, time

DB_CONN = ["psql", "-h", "/tmp", "-p", "5432", "-U", "fashion_ai", "-d", "fashion_ai"]
EMBEDDING_API = "https://maas.qianwenaiapi.com/compatible-mode/v1/embeddings"
EMBEDDING_MODEL = "text-embedding-v3"
BATCH_SIZE = 8
REQUEST_TIMEOUT = 15

ENV_FILE = "/Users/lionfan/projects/fashion-ai-platform/.env"
api_key = None
with open(ENV_FILE) as f:
    for line in f:
        if line.startswith("DASHSCOPE_API_KEY="):
            api_key = line.split("=", 1)[1].strip()
            break
if not api_key:
    print("ERROR: DASHSCOPE_API_KEY not found")
    sys.exit(1)

def psql(query):
    r = subprocess.run(DB_CONN + ["-A", "-t", "-c", query],
        capture_output=True, text=True, timeout=30)
    if r.returncode != 0:
        raise RuntimeError(f"psql error: {r.stderr.strip()}")
    if not r.stdout.strip():
        return []
    return [line.split("|") for line in r.stdout.strip().split("\n")]

def psql_one(query):
    rows = psql(query)
    return rows[0][0] if rows else None

def get_embeddings(texts):
    payload = json.dumps({"model": EMBEDDING_MODEL, "input": texts}, ensure_ascii=False)
    r = subprocess.run(
        ["curl", "-s", "-X", "POST", EMBEDDING_API,
         "-H", f"Authorization: Bearer {api_key}",
         "-H", "Content-Type: application/json",
         "-d", payload, "--noproxy", "*", "-m", str(REQUEST_TIMEOUT)],
        capture_output=True, text=True, timeout=REQUEST_TIMEOUT + 5)
    if r.returncode != 0:
        raise RuntimeError(f"curl exit {r.returncode}")
    d = json.loads(r.stdout)
    if "error" in d:
        raise RuntimeError(f"API error: {d['error']['message'][:80]}")
    return [item["embedding"] for item in sorted(d["data"], key=lambda x: x["index"])]

total = int(psql_one("SELECT COUNT(*) FROM fabrics WHERE description_embedding IS NULL") or 0)
print(f"待处理面料: {total} 条")
if total == 0:
    print("全部面料已有向量嵌入")
    sys.exit(0)

all_rows = psql("""
    SELECT id, description FROM fabrics
    WHERE description IS NOT NULL AND description != '' AND description_embedding IS NULL
    ORDER BY id LIMIT 200
""")
print(f"读取到 {len(all_rows)} 条")

updated, failed, errors = 0, 0, []
for i in range(0, len(all_rows), BATCH_SIZE):
    batch = all_rows[i:i+BATCH_SIZE]
    ids, texts = [r[0] for r in batch], [r[1][:2000] for r in batch]
    try:
        embs = get_embeddings(texts)
        for fid, emb in zip(ids, embs):
            vec = "[" + ",".join(str(x) for x in emb) + "]"
            psql(f"UPDATE fabrics SET description_embedding = '{vec}'::vector WHERE id = '{fid}'")
        updated += len(batch)
        print(f"  OK 批次{i//BATCH_SIZE+1}/{len(all_rows)//BATCH_SIZE+1}: {len(batch)}条 (累计{updated}/{total})")
    except Exception as e:
        failed += len(batch)
        errors.append(str(e)[:80])
        print(f"  FAIL 批次{i//BATCH_SIZE+1}: {e}")
        time.sleep(2)

print(f"\n完成: 成功 {updated}, 失败 {failed}")
for e in errors[:3]: print(f"  {e}")

filled = int(psql_one("SELECT COUNT(*) FROM fabrics WHERE description_embedding IS NOT NULL") or 0)
print(f"验证: {filled}/{total} 条已有向量嵌入")
for row in psql("SELECT name, left(description,40), dim(description_embedding) FROM fabrics WHERE description_embedding IS NOT NULL LIMIT 3"):
    print(f"  [{row[0]}] desc={row[1]}... dim={row[2]}")
