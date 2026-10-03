import { expect, test, type Page } from "@playwright/test"

import { toAutomationIdSegment } from "../../erp-client/lib/automation-id"
import {
    addDeliveryTrackingEntry,
    fillAllDeliveryLineTracking,
    fillDeliveryTrackingEntries,
    uploadAcceptanceEvidence,
} from "./fulfillment"

/** 模拟当前生产的按销售明细输入、承运方下拉和添加按钮，不连接应用。 */
async function renderDeliveryForm(page: Page): Promise<void> {
    const lineIds = ["sales_line_A", "sales_line_B"]
    await page.setContent(`
        <div aria-label="供应商直发表单">
            ${lineIds
                .map((lineId) => {
                    const prefix = `fulfillment-operations-direct-form-line-${toAutomationIdSegment(lineId)}`
                    return `<form id="${prefix}-form" data-line-id="${lineId}">
                    <textarea id="${prefix}-tracking-no"></textarea>
                    <input id="${prefix}-carrier" role="combobox" aria-expanded="false">
                    <button id="${prefix}-carrier-clear" type="button">清空承运方</button>
                    <div id="${prefix}-popup" data-slot="combobox-content" hidden>
                        <div role="listbox" id="${prefix}-options">
                            <div id="${prefix}-carrier-option-shun-feng" role="option" data-slot="combobox-item">顺丰速运</div>
                            <div id="${prefix}-carrier-option-huo-la-la" role="option" data-slot="combobox-item">货拉拉</div>
                        </div>
                    </div>
                    <button id="${prefix}-add" type="submit">添加物流号</button>
                </form>`
                })
                .join("")}
        </div>
        <script>
            window.entries = [];
            for (const form of document.querySelectorAll('form')) {
                const prefix = form.id.slice(0, -'-form'.length);
                const tracking = document.getElementById(prefix + '-tracking-no');
                const carrier = document.getElementById(prefix + '-carrier');
                const popup = document.getElementById(prefix + '-popup');
                const filterOptions = (query) => {
                    for (const option of popup.querySelectorAll('[role="option"]')) {
                        option.hidden = query && option.textContent !== query;
                    }
                };
                carrier.addEventListener('click', () => {
                    carrier.setAttribute('aria-expanded', 'true');
                    carrier.setAttribute('aria-controls', prefix + '-options');
                    popup.setAttribute('data-open', '');
                    popup.hidden = false;
                    filterOptions(carrier.dataset.selection || carrier.value);
                });
                carrier.addEventListener('input', () => filterOptions(carrier.dataset.selection || carrier.value));
                for (const option of popup.querySelectorAll('[role="option"]')) {
                    option.addEventListener('click', () => {
                        carrier.value = option.textContent;
                        carrier.dataset.selection = carrier.value;
                        carrier.setAttribute('aria-expanded', 'false');
                        popup.removeAttribute('data-open');
                        popup.hidden = true;
                    });
                }
                document.getElementById(prefix + '-carrier-clear').addEventListener('click', () => {
                    carrier.value = '';
                    delete carrier.dataset.selection;
                    filterOptions('');
                });
                form.addEventListener('submit', (event) => {
                    event.preventDefault();
                    window.entries.push({ salesOrderLineId: form.dataset.lineId, trackingNo: tracking.value, carrier: carrier.value });
                    tracking.value = '';
                });
            }
        </script>
    `)
}

type RecordedTrackingEntry = {
    salesOrderLineId: string
    trackingNo: string
    carrier: string
}

function recordedEntries(page: Page): Promise<RecordedTrackingEntry[]> {
    return page.evaluate(
        () =>
            (window as unknown as { entries: RecordedTrackingEntry[] }).entries,
    )
}

test("按真实销售行添加不同包裹和承运方，保持跨明细关联", async ({ page }) => {
    await renderDeliveryForm(page)
    await fillDeliveryTrackingEntries(page, {
        kind: "direct",
        entries: [
            {
                salesOrderLineId: "sales_line_A",
                trackingNo: "SF-A",
                carrier: "顺丰速运",
            },
            {
                salesOrderLineId: "sales_line_A",
                trackingNo: "HLL-A",
                carrier: "货拉拉",
            },
            {
                salesOrderLineId: "sales_line_B",
                trackingNo: "SF-B",
                carrier: "顺丰速运",
            },
        ],
    })
    const inputId = await addDeliveryTrackingEntry(page, {
        kind: "direct",
        lineIndex: 0,
        trackingNo: "A-UNKNOWN-CARRIER",
    })
    expect(inputId).toBe(
        "fulfillment-operations-direct-form-line-sales-line-a-tracking-no",
    )
    expect(await recordedEntries(page)).toEqual([
        {
            salesOrderLineId: "sales_line_A",
            trackingNo: "SF-A",
            carrier: "顺丰速运",
        },
        {
            salesOrderLineId: "sales_line_A",
            trackingNo: "HLL-A",
            carrier: "货拉拉",
        },
        {
            salesOrderLineId: "sales_line_B",
            trackingNo: "SF-B",
            carrier: "顺丰速运",
        },
        {
            salesOrderLineId: "sales_line_A",
            trackingNo: "A-UNKNOWN-CARRIER",
            carrier: "",
        },
    ])
})

test("同一包裹承载两条实际明细时，逐行添加相同物流号", async ({ page }) => {
    await renderDeliveryForm(page)
    await fillAllDeliveryLineTracking(page, {
        kind: "direct",
        trackingNo: "SHARED-PACKAGE",
        carrier: "货拉拉",
    })
    expect(await recordedEntries(page)).toEqual([
        {
            salesOrderLineId: "sales_line_A",
            trackingNo: "SHARED-PACKAGE",
            carrier: "货拉拉",
        },
        {
            salesOrderLineId: "sales_line_B",
            trackingNo: "SHARED-PACKAGE",
            carrier: "货拉拉",
        },
    ])
})

test("签收上传以表单文件名回显确认成功，原生文件输入清空仍可验收", async ({
    page,
}) => {
    await page.setContent(`
        <div role="dialog" aria-label="登记客户验收">
            <input id="sales-orders-acceptance-evidence-input" type="file" accept="application/pdf,image/*">
            <span id="selected-file"></span>
        </div>
        <script>
            document.querySelector('input').addEventListener('change', (event) => {
                document.getElementById('selected-file').textContent = event.target.files[0].name;
                event.target.value = '';
            });
        </script>
    `)
    await uploadAcceptanceEvidence(page)
    await expect(page.locator("#selected-file")).toHaveText(
        "sample-contract.pdf",
    )
    await expect(
        page.locator("#sales-orders-acceptance-evidence-input"),
    ).toHaveValue("")
})
