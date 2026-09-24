"""读取 Helm 环境 values；JSON 是 Helm 支持的 values 格式，仅需 Python 标准库。"""
import json
import os
from pathlib import Path
import shlex


def configuration():
    environment = os.environ.get("DEPLOY_ENV", "")
    namespaces = {"production": "prod", "test": "test"}
    if environment not in namespaces:
        raise ValueError("DEPLOY_ENV 必须显式设置为 production 或 test")
    chart = Path(__file__).resolve().parents[1] / "helm/erp"
    values = json.loads((chart / f"environments/{environment}.json").read_text())
    if values["environment"] != environment or values["namespace"] != namespaces[environment]:
        raise ValueError("环境与命名空间映射不一致")
    other = "test" if environment == "production" else "production"
    other_values = json.loads((chart / f"environments/{other}.json").read_text())
    hosts = {values["ingress"][key] for key in ("apiHost", "webHost")}
    other_hosts = {other_values["ingress"][key] for key in ("apiHost", "webHost")}
    if len(hosts) != 2 or hosts & other_hosts:
        raise ValueError("生产与测试、API 与管理端不得共用域名")
    return values


if __name__ == "__main__":
    values = configuration()
    variables = {
        "KUBE_NAMESPACE": values["namespace"],
        "HELM_RELEASE": "erp",
        "NEXT_PUBLIC_API_BASE_URL": "https://" + values["ingress"]["apiHost"],
        "WEB_URL": "https://" + values["ingress"]["webHost"],
        "CLB_ID": values["ingress"]["clbId"],
        "TLS_SECRET": values["ingress"]["tlsSecret"],
        "API_CONFIG_SECRET": values["api"]["configSecret"],
    }
    for name, value in variables.items():
        print(f"export {name}={shlex.quote(value)}")
