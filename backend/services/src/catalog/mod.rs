//! 域 D10 `catalog`：product_category、product_brand、unit_of_measure、sku_attribute、product(+_revision)、sku(+_revision)、voucher_category_profile_revision 等（页面：W14）。P0 骨架占位；P3 填充服务与事务（Service 不跨域依赖，跨域只调对方 Repository）。

pub mod dto;
