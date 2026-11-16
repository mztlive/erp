//! 客户资料写命令输入与主体值类型的映射。
use erp_customer::AddressType;
use erp_party::AddressType as PartyAddressType;
/// 把客户资料地址类型映射为主体地址类型。
///
/// # 参数
/// * `address_type` - 客户资料地址类型。
///
/// # 返回
/// 返回同名的主体地址类型。
///
/// # 错误
/// 不返回错误。
pub(super) fn party_address_type(address_type: AddressType) -> PartyAddressType {
    match address_type {
        AddressType::Registered => PartyAddressType::Registered,
        AddressType::Operating => PartyAddressType::Operating,
        AddressType::Fulfillment => PartyAddressType::Fulfillment,
    }
}
