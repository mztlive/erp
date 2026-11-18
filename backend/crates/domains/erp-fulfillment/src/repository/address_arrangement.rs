//! 独立履约收货安排集合，按销售单唯一读取。
use mongodb::Database;
use persistence_core::Repository;

use crate::entity::address_arrangement::AddressArrangement;
/// 履约安排拥有集合访问器。
pub trait AddressArrangementExt {
    const ADDRESS_ARRANGEMENTS: &'static str = "fulfillment_address_arrangements";
    /// 返回地址安排仓储。
    /// 绑定独立集合的仓储。
    fn address_arrangements(&self) -> Repository<'_, AddressArrangement>;
}
impl AddressArrangementExt for Database {
    fn address_arrangements(&self) -> Repository<'_, AddressArrangement> {
        Repository::new(self, Self::ADDRESS_ARRANGEMENTS)
    }
}
