//! 演示主数据的对外名称。稳定编号仍用 DEMO-MD，列表里显示这些名称。

use super::plan::{DemoKind, DemoStep};

pub(super) struct PartyFace {
    pub legal_name: &'static str,
    pub short_name: &'static str,
    pub contact: &'static str,
    pub phone: &'static str,
    pub address: &'static str,
    pub bank: &'static str,
}

const CUSTOMERS: &[PartyFace] = &[
    face("杭州市总工会", "杭州工会", "王敏", "13800001001", "杭州市上城区解放路18号工会大楼", ""),
    face("深圳市南山区总工会", "南山工会", "李强", "13800001002", "深圳市南山区海德三道南海大道西", ""),
    face("成都市总工会", "成都工会", "张丽", "13800001003", "成都市青羊区东城根上街78号", ""),
    face("南京市建邺区总工会", "建邺工会", "陈浩", "13800001004", "南京市建邺区江东中路269号", ""),
    face("武汉市武昌区总工会", "武昌工会", "刘洋", "13800001005", "武汉市武昌区中山路307号", ""),
    face("西安市雁塔区总工会", "雁塔工会", "赵静", "13800001006", "西安市雁塔区慈恩西路66号", ""),
    face("苏州工业园区总工会", "园区工会", "周凯", "13800001007", "苏州市工业园区现代大道999号", ""),
    face("青岛市市南区总工会", "市南工会", "孙婷", "13800001008", "青岛市市南区香港中路18号", ""),
    face("长沙市岳麓区总工会", "岳麓工会", "吴磊", "13800001009", "长沙市岳麓区桐梓坡路258号", ""),
    face("厦门市思明区总工会", "思明工会", "郑琳", "13800001010", "厦门市思明区湖滨北路61号", ""),
    face("合肥市蜀山区总工会", "蜀山工会", "黄伟", "13800001011", "合肥市蜀山区长江西路189号", ""),
    face("宁波市海曙区总工会", "海曙工会", "徐佳", "13800001012", "宁波市海曙区中山西路1号", ""),
    face("浙江移动通信有限公司工会", "浙江移动工会", "马超", "13800001013", "杭州市江干区解放东路19号", ""),
    face("上海建行职工工会", "上海建行工会", "冯雪", "13800001014", "上海市浦东新区陆家嘴环路900号", ""),
    face("深圳华为职工服务中心", "华为职工服务", "韩冰", "13800001015", "深圳市龙岗区坂田华为基地", ""),
    face("比亚迪汽车工业有限公司工会", "比亚迪工会", "曹阳", "13800001016", "深圳市坪山区比亚迪路3009号", ""),
    face("宁德时代职工工会", "宁德时代工会", "邓凯", "13800001017", "宁德市蕉城区漳湾镇新港路2号", ""),
    face("美的集团职工服务中心", "美的职工服务", "彭丽", "13800001018", "佛山市顺德区北滘镇美的大道6号", ""),
    face("海尔集团工会", "海尔工会", "蒋涛", "13800001019", "青岛市崂山区海尔路1号", ""),
    face("顺丰速运职工工会", "顺丰工会", "蔡敏", "13800001020", "深圳市福田区新洲十一街万基大厦", ""),
    face("京东世纪贸易有限公司工会", "京东工会", "潘军", "13800001021", "北京市大兴区科创十一街18号", ""),
    face("阿里巴巴（中国）有限公司工会", "阿里工会", "董倩", "13800001022", "杭州市余杭区文一西路969号", ""),
    face("腾讯科技职工服务中心", "腾讯职工服务", "袁航", "13800001023", "深圳市南山区海天二路33号", ""),
    face(
        "中国平安财产保险股份有限公司工会",
        "平安产险工会",
        "许宁",
        "13800001024",
        "深圳市福田区益田路5033号",
        "",
    ),
];

const SUPPLIERS: &[PartyFace] = &[
    face(
        "义乌市锦礼实业有限公司",
        "锦礼实业",
        "林建国",
        "13900002001",
        "浙江省义乌市福田街道诚信大道88号",
        "中国工商银行义乌分行",
    ),
    face(
        "广州市穗选食品有限公司",
        "穗选食品",
        "何美玲",
        "13900002002",
        "广州市白云区江高镇私企区夏花二路",
        "中国建设银行广州白云支行",
    ),
    face(
        "温州市恒达日用品有限公司",
        "恒达日用",
        "叶志强",
        "13900002003",
        "温州市瓯海区梧田街道月乐西街",
        "招商银行温州分行",
    ),
    face(
        "东莞市宏盛包装有限公司",
        "宏盛包装",
        "罗海燕",
        "13900002004",
        "东莞市厚街镇家具大道168号",
        "中国农业银行东莞厚街支行",
    ),
    face(
        "泉州市正山茶业有限公司",
        "正山茶业",
        "苏文斌",
        "13900002005",
        "泉州市安溪县城厢镇茶都大道",
        "中国银行泉州分行",
    ),
    face(
        "成都市天府粮油贸易有限公司",
        "天府粮油",
        "唐雪梅",
        "13900002006",
        "成都市新都区物流大道66号",
        "中国工商银行成都新都支行",
    ),
    face(
        "杭州市西湖龙井茶业有限公司",
        "西湖龙井茶业",
        "金明",
        "13900002007",
        "杭州市西湖区龙井路1号",
        "杭州银行湖滨支行",
    ),
    face(
        "苏州市园林丝绸有限公司",
        "园林丝绸",
        "沈怡",
        "13900002008",
        "苏州市姑苏区平江路128号",
        "中国建设银行苏州姑苏支行",
    ),
    face(
        "宁波市海味水产有限公司",
        "海味水产",
        "朱斌",
        "13900002009",
        "宁波市北仑区霞浦街道水产路",
        "宁波银行北仑支行",
    ),
    face(
        "青岛市崂山矿泉水业有限公司",
        "崂山矿泉",
        "吕娜",
        "13900002010",
        "青岛市崂山区沙子口街道",
        "青岛银行崂山支行",
    ),
    face(
        "北京市京华文具有限公司",
        "京华文具",
        "高飞",
        "13900002011",
        "北京市通州区马驹桥物流园",
        "中国工商银行北京通州支行",
    ),
    face(
        "上海市申城家纺有限公司",
        "申城家纺",
        "宋雅",
        "13900002012",
        "上海市青浦区华新镇华志路",
        "上海银行青浦支行",
    ),
    face(
        "深圳市前海数码配件有限公司",
        "前海数码",
        "梁宇",
        "13900002013",
        "深圳市南山区前海桂湾片区",
        "招商银行深圳前海支行",
    ),
    face(
        "中山市灯都照明有限公司",
        "灯都照明",
        "谢婷",
        "13900002014",
        "中山市古镇镇新兴大道",
        "中国农业银行中山古镇支行",
    ),
    face(
        "佛山市顺德厨具有限公司",
        "顺德厨具",
        "胡军",
        "13900002015",
        "佛山市顺德区勒流街道工业路",
        "中国建设银行佛山顺德支行",
    ),
    face(
        "厦门市鼓浪屿工艺品有限公司",
        "鼓浪屿工艺",
        "方洁",
        "13900002016",
        "厦门市思明区湖滨南路",
        "厦门银行思明支行",
    ),
];

const BRANDS: &[&str] = &["山野良品", "果壳日记", "百味轩", "洽果", "福记", "旺礼", "金穗", "鲁味"];

const CATEGORIES: &[&str] = &[
    "粮油调味",
    "休闲食品",
    "茶叶冲饮",
    "酒水饮料",
    "家清纸品",
    "家居床品",
    "数码小电",
    "厨房用具",
    "生鲜礼盒",
    "节日礼盒",
];

const WAREHOUSES: &[(&str, &str, &str)] = &[
    ("杭州下沙仓", "杭州市钱塘区下沙街道文泽路88号", "仓管周宁"),
    ("广州黄埔仓", "广州市黄埔区开发大道358号", "仓管陈立"),
    ("成都双流仓", "成都市双流区物流大道166号", "仓管刘畅"),
    ("上海外高桥仓", "上海市浦东新区外高桥保税区美盛路168号", "仓管王磊"),
    ("武汉东西湖仓", "武汉市东西湖区走马岭食品一路", "仓管赵敏"),
    ("西安浐灞仓", "西安市浐灞生态区灞柳一路", "仓管孙浩"),
];

const PRODUCTS: &[(&str, &str)] = &[
    ("山野混合坚果礼盒", "1.2千克"),
    ("每日坚果分享装", "750克"),
    ("黄油曲奇礼盒", "608克"),
    ("混合果干罐装", "500克"),
    ("洽果香瓜子礼罐", "800克"),
    ("福记酥心糖礼盒", "1千克"),
    ("旺礼仙贝家庭装", "480克"),
    ("金穗调和油", "5升"),
    ("鲁味压榨花生油", "1.8升"),
    ("五常稻花香大米", "5千克"),
    ("有机杂粮礼盒", "2.5千克"),
    ("西湖龙井", "250克"),
    ("安溪铁观音礼盒", "500克"),
    ("普洱熟茶饼", "357克"),
    ("洗衣液家庭套装", "3千克×2"),
    ("抽纸整箱", "24包"),
    ("不锈钢保温杯", "480毫升"),
    ("全棉床品四件套", "1.8米"),
    ("无线蓝牙耳机", "标准装"),
    ("护眼台灯", "国AA级"),
    ("不粘炒锅", "32厘米"),
    ("特级酱油礼盒", "500毫升×2"),
    ("特级初榨橄榄油", "750毫升"),
    ("纯牛奶礼箱", "250毫升×12"),
];

const fn face(
    legal_name: &'static str,
    short_name: &'static str,
    contact: &'static str,
    phone: &'static str,
    address: &'static str,
    bank: &'static str,
) -> PartyFace {
    PartyFace { legal_name, short_name, contact, phone, address, bank }
}

/// 列表和单据上显示的名称。
pub(super) fn label(step: &DemoStep) -> &'static str {
    let index = index_of(step.ordinal);
    match step.kind {
        DemoKind::Unit => super::plan::unit_spec(&step.key).map(|(name, _)| name).unwrap_or("件"),
        DemoKind::Brand => BRANDS.get(index).copied().unwrap_or(BRANDS[0]),
        DemoKind::Category => CATEGORIES.get(index).copied().unwrap_or(CATEGORIES[0]),
        DemoKind::Warehouse => WAREHOUSES.get(index).map(|row| row.0).unwrap_or(WAREHOUSES[0].0),
        DemoKind::Customer => customer(step.ordinal).legal_name,
        DemoKind::Supplier => supplier(step.ordinal).legal_name,
        DemoKind::Product => PRODUCTS.get(index).map(|row| row.0).unwrap_or(PRODUCTS[0].0),
    }
}

pub(super) fn customer(ordinal: u16) -> &'static PartyFace {
    &CUSTOMERS[index_of(ordinal).min(CUSTOMERS.len() - 1)]
}

pub(super) fn supplier(ordinal: u16) -> &'static PartyFace {
    &SUPPLIERS[index_of(ordinal).min(SUPPLIERS.len() - 1)]
}

pub(super) fn product_spec(ordinal: u16) -> &'static str {
    PRODUCTS.get(index_of(ordinal)).map(|row| row.1).unwrap_or(PRODUCTS[0].1)
}

pub(super) fn warehouse(ordinal: u16) -> (&'static str, &'static str, &'static str) {
    let row = WAREHOUSES.get(index_of(ordinal)).unwrap_or(&WAREHOUSES[0]);
    (row.0, row.1, row.2)
}

pub(super) fn supplier_bank(ordinal: u16) -> &'static str {
    supplier(ordinal).bank
}

fn index_of(ordinal: u16) -> usize {
    usize::from(ordinal.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::super::plan::{demo_steps, planned_counts};
    use super::{BRANDS, CATEGORIES, CUSTOMERS, PRODUCTS, SUPPLIERS, WAREHOUSES, label};

    #[test]
    fn catalog_covers_every_planned_row_without_placeholder_words() {
        assert_eq!(CUSTOMERS.len(), planned_counts().customer as usize);
        assert_eq!(SUPPLIERS.len(), planned_counts().supplier as usize);
        assert_eq!(BRANDS.len(), planned_counts().brand as usize);
        assert_eq!(CATEGORIES.len(), planned_counts().category as usize);
        assert_eq!(WAREHOUSES.len(), planned_counts().warehouse as usize);
        assert_eq!(PRODUCTS.len(), planned_counts().product as usize);
        for step in demo_steps() {
            let name = label(&step);
            assert!(!name.contains('演'), "{name}");
            assert!(!name.is_empty());
        }
    }
}
