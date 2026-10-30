#!/usr/bin/env bash
# 启动本地调度中心及其专用 MySQL；不启动 ERP、不触发业务任务。
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
state_dir="$repo_root/.local/xxl-job"
umask 077
mkdir -p "$state_dir"
python3 - "$state_dir" <<'PY'
import hashlib
import secrets
import sys
from pathlib import Path

state = Path(sys.argv[1])
env = state / "credentials.env"
if not env.exists():
    credentials = {
        "XXL_MYSQL_ROOT_PASSWORD": secrets.token_hex(24),
        "XXL_MYSQL_PASSWORD": secrets.token_hex(24),
        "XXL_ADMIN_PASSWORD": secrets.token_hex(8),
        "XXL_ACCESS_TOKEN": secrets.token_hex(24),
    }
    env.write_text("".join(f"{key}={value}\n" for key, value in credentials.items()))
values = dict(line.split("=", 1) for line in env.read_text().splitlines() if line)
password = hashlib.md5(values["XXL_ADMIN_PASSWORD"].encode()).hexdigest()
(state / "01-local.sql").write_text(f"""SET NAMES utf8mb4;
USE xxl_job;
START TRANSACTION;
UPDATE xxl_job_user SET password='{password}' WHERE username='admin';
UPDATE xxl_job_group SET app_name='erp-local', title='ERP本地执行器', update_time=NOW() WHERE id=1;
UPDATE xxl_job_info SET job_desc='供应商履约补查', author='ERP',
    executor_handler='supplier_tracking_scan', executor_block_strategy='DISCARD_LATER',
    schedule_conf='0/5 * * * * ?', trigger_status=0 WHERE id=1;
INSERT INTO xxl_job_info (id, job_group, job_desc, author, alarm_email, add_time, update_time,
    schedule_type, schedule_conf, misfire_strategy, executor_route_strategy, executor_handler,
    executor_param, executor_block_strategy, executor_timeout, executor_fail_retry_count,
    glue_type, glue_source, glue_remark, glue_updatetime, child_jobid, trigger_status)
VALUES (2, 1, '供应商目录计划', 'ERP', '', NOW(), NOW(), 'CRON', '0/5 * * * * ?',
    'DO_NOTHING', 'FIRST', 'supplier_catalog_schedule', '', 'DISCARD_LATER', 0, 0,
    'BEAN', '', '', NOW(), '', 0);
COMMIT;
""")
(state / "erp-config.toml").write_text(f"""[xxl_job]
admin_url = "http://127.0.0.1:18080/xxl-job-admin"
executor_url = "http://host.docker.internal:10002"
app_name = "erp-local"
access_token = "{values['XXL_ACCESS_TOKEN']}"
""")
PY
docker compose --env-file "$state_dir/credentials.env" \
    -f "$repo_root/deploy/local/xxl-job/compose.yaml" up -d
for ((attempt=0; attempt<90; attempt++)); do
    if curl --fail --silent --max-time 2 http://127.0.0.1:18080/xxl-job-admin/toLogin >/dev/null; then
        python3 - "$state_dir/credentials.env" <<'PYLOGIN'
import json
import sys
import urllib.parse
import urllib.request
from pathlib import Path
values = dict(line.split("=", 1) for line in Path(sys.argv[1]).read_text().splitlines() if line)
body = urllib.parse.urlencode({"userName": "admin", "password": values["XXL_ADMIN_PASSWORD"]}).encode()
with urllib.request.urlopen("http://127.0.0.1:18080/xxl-job-admin/login", body, timeout=10) as response:
    if json.load(response).get("code") != 200:
        raise SystemExit("本地 admin 登录校验失败；请核对初始化日志或 credentials.env，禁止清空数据卷重试。")
PYLOGIN
        printf 'XXL-JOB 已启动：http://127.0.0.1:18080/xxl-job-admin/\n账号：admin；密码见 %s/credentials.env 的 XXL_ADMIN_PASSWORD。\nERP 配置片段：%s/erp-config.toml\n首次创建的两个任务默认为停用；Worker 就绪后手动测试。\n' "$state_dir" "$state_dir"
        exit 0
    fi
    sleep 2
done
echo 'XXL-JOB 登录页未就绪，请检查 erp-xxl-local 的容器日志。' >&2
exit 1
