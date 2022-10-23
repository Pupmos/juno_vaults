#[cfg(not(feature = "library"))]
// The Essentials
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, from_binary, Binary, Deps, DepsMut, Env, 
    MessageInfo, Response, StdResult, Addr, QuerierWrapper// coin, Coin, Uint128
};
use cw2::set_contract_version;
use serde::de::DeserializeOwned;
use serde::Serialize;

// The Commons
use crate::msg::*;
use crate::state::*;
use crate::error::ContractError;
use crate::execute::*;
use crate::query::*;
use std::str;

// The Personals
use cw20::{Balance, Cw20CoinVerified, Cw20ReceiveMsg, BalanceResponse}; //Cw20ExecuteMsg, BalanceResponse}; // Cw20Coin
use cw721::{
    Cw721ReceiveMsg, Cw721QueryMsg, AllNftInfoResponse, 
    OwnerOfResponse, NftInfoResponse, Expiration};
//use cw721_base::Extension;
//use cosmwasm_std::{SubMsg, WasmMsg, BankMsg, CosmosMsg};
//use cosmwasm_std::{QueryRequest, WasmQuery};

// Contract name used for migration
const CONTRACT_NAME: &str = "crates.io:cyberswap_nft";
// Contract version thats used for migration
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

////////////////////////////////////////////////////////////////////////////////////////

//////~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
///////////~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
////////////// Instantiate
///////////~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let admin = msg.admin.unwrap_or_else(|| info.sender.to_string());

    let validated = deps.api.addr_validate(&admin)?;

    // Hardcoded whitelist
    let native_whitelist: Vec<(String, String)> = {
        let mut nw = vec![
            ("JUNO".to_string(), "ujunox".to_string()),
            ("ATOM".to_string(), "ibc/C4CFF46FD6DE35CA4CF4CE031E643C8FDC9BA4B99AE598E9B0ED98FE3A2319F9".to_string()),
            ("USDC".to_string(), "ibc/EAC38D55372F38F1AFD68DF7FE9EF762DCF69F26520643CF3F9D292A738D8034".to_string()),
            ("OSMO".to_string(), "ibc/ED07A3391A112B175915CD8FAF43A2DA8E4790EDE12566649D0C2F97716B8518".to_string()),
            ("STARS".to_string(), "ibc/F6B367385300865F654E110976B838502504231705BAC0849B0651C226385885".to_string()),
            ("SCRT".to_string(), "ibc/B55B08EF3667B0C6F029C2CC9CAA6B00788CF639EBB84B34818C85CBABA33ABD".to_string()),
        ];
        nw.reserve(2);
        nw
    };
    let cw20_whitelist: Vec<(String, Addr)> = {
        let mut cw20_wl = vec![
            // Fake cw20's for tests
            //("your superbizdevalphamarketingvcsuperiorburncoin here".to_string(), deps.api.addr_validate("")?),
            ("CSUNO".to_string(), deps.api.addr_validate("juno18zy5ywkkpqlfpjc2mz8fr5jlqvwktc9sz6avlqly5e9qqwmde7tsg8efad")?),
            ("CSTWO".to_string(), deps.api.addr_validate("juno1p3ge77f9kf5hwng09s2qggldkf7rn5yvu4vwvctykf05p8krzu9q25235j")?),
            ("CSTRE".to_string(), deps.api.addr_validate("juno1cm4etasezlaqlr4vltju5s3sr7m588j0tqj8le58ny78j0v3suwqhcjs3l")?),
        ];
        cw20_wl.reserve(2);
        cw20_wl
    };
        
    CONFIG.save(deps.storage, &Config{
        admin: validated,
        whitelist_native: native_whitelist,
        whitelist_cw20: cw20_whitelist,
        removal_queue_native: None,
        removal_queue_cw20: None,
    })?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("admin", admin)
    )
}


//////~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
///////////~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
////////////// Execute
///////////~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute<T: DeserializeOwned>(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {

    match msg {

        // ~~~~~~~~~~~~~~~~~~~~~~~~~ Sudo
        ExecuteMsg::AddToWhitelist { new_denom, marker } => add_to_whitelist(deps, env, &info.sender, new_denom, marker),
        ExecuteMsg::AddToRemovalQueue { denom, marker } => add_to_removal_queue(deps, env, &info.sender, denom, marker),
        ExecuteMsg::ClearRemovalQueue {} => clear_removal_queue(deps, env, &info.sender),

        // ~~~~~~~~~~~~~~~~~~~~~~~~~ Wrapper cw20/cw721
        ExecuteMsg::Receive(receive_msg) => execute_receive(deps, env, info, receive_msg),
        ExecuteMsg::ReceiveNft(receive_nft_msg) => execute_receive_nft::<T>(deps, env, info, receive_nft_msg),

        // ~~~~~~~~~~~~~~~~~~~~~~~~~ Create Listing
        ExecuteMsg::CreateListing { create_msg } => execute_create_listing(deps, &info.sender, Balance::from(info.funds), create_msg),        

        // ~~~~~~~~~~~~~~~~~~~~~~~~~ Edit Listing
        // Adding native tokens to a sale
        ExecuteMsg::AddFundsToSaleNative { listing_id } => execute_add_funds_to_sale(deps, Balance::from(info.funds), &info.sender, listing_id),
        // Changing price of a sale
        ExecuteMsg::ChangeAsk { listing_id, new_ask } => execute_change_ask(deps, &info.sender, listing_id, new_ask),
        // Can only remove if listing has not yet been finalized
        ExecuteMsg::RemoveListing { listing_id } => execute_remove_listing(deps, &info.sender, listing_id),
        // Finalizes listing for sale w/ expiration
        ExecuteMsg::Finalize { listing_id, seconds } => execute_finalize(deps, env, &info.sender, listing_id, seconds),
        // Refunds unpurchased listing if expired
        ExecuteMsg::RefundExpired { listing_id } => execute_refund(deps, env, &info.sender, listing_id),

        // ~~~~~~~~~~~~~~~~~~~~~~~~~ Buckets
        // Create a bucket
        ExecuteMsg::CreateBucket { bucket_id } => execute_create_bucket(deps, Balance::from(info.funds), &info.sender, bucket_id),
        // Add funds to bucket
        ExecuteMsg::AddToBucket { bucket_id } => execute_add_to_bucket(deps, Balance::from(info.funds), &info.sender, bucket_id),
        // Remove and delete a bucket
        ExecuteMsg::RemoveBucket { bucket_id } => execute_withdraw_bucket(deps, &info.sender, bucket_id),

        // ~~~~~~~~~~~~~~~~~~~~~~~~~ Purchasing
        ExecuteMsg::BuyListing { listing_id, bucket_id } => execute_buy_listing(deps, env, &info.sender, listing_id, bucket_id),
        ExecuteMsg::WithdrawPurchased { listing_id } => execute_withdraw_purchased(deps, &info.sender, listing_id),

    }

}

// "Filter" for cw20 tokens
pub fn execute_receive(
    deps: DepsMut, 
    _env: Env, 
    info: MessageInfo, 
    wrapper: Cw20ReceiveMsg, 
) -> Result<Response, ContractError> {

    let msg: ReceiveMsg = from_binary(&wrapper.msg)?;
    
    let user_wallet = &deps.api.addr_validate(&wrapper.sender)?;

    // Query the sending contract to get user's balance & verify it's >= wrapper.amount
    let bal_res: BalanceResponse = deps
        .querier
        .query_wasm_smart(
            &info.sender, 
            &cw20::Cw20QueryMsg::Balance {address: wrapper.sender},
        )?;
    
    if bal_res.balance <= wrapper.amount {
        return Err(ContractError::ToDo {});
    };

    let balance = Balance::Cw20(Cw20CoinVerified {
        //cw20 contract this message was sent from
        address: info.sender.clone(),
        amount: wrapper.amount,
    });

    match msg {
        // Create listing with Cw20's initially
        ReceiveMsg::CreateListingCw20 { create_msg } => execute_create_listing_cw20(deps, user_wallet, &info.sender, balance, create_msg),
        // Add Cw20's to sale
        ReceiveMsg::AddFundsToSaleCw20 { listing_id } => execute_add_funds_to_sale(deps, balance, user_wallet, listing_id),
        // Create Bucket with Cw20's initially
        ReceiveMsg::CreateBucketCw20 { bucket_id } => execute_create_bucket(deps, balance, user_wallet, bucket_id),
        // Add Cw20's to bucket
        ReceiveMsg::AddToBucketCw20 { bucket_id } => execute_add_to_bucket(deps, balance, user_wallet, bucket_id),

    }
}

// "Filter" for NFTs
pub fn execute_receive_nft<T: DeserializeOwned>(
    deps: DepsMut, 
    env: Env, 
    info: MessageInfo, 
    wrapper: Cw721ReceiveMsg, 
) -> Result<Response, ContractError> {

    // wrapper.token_id = token_id of the NFT
    // wrapper.msg = binary message sent with NFT
    // wrapper.sender = user wallet that sent NFT
    // info.sender = cw721 contract of the NFT

    // Query the sending contract to gather information about the NFT sent
    let all_res: AllNftInfoResponse<T> = deps
        .querier
        .query_wasm_smart(
            &info.sender, 
            &cw721::Cw721QueryMsg::AllNftInfo {token_id: wrapper.token_id.clone(), include_expired: None},
        )?;

    let owner_res = all_res.access;

    // Check that wrapper.sender is the owner of this Token ID according to the NFT sending contract
    if deps.api.addr_validate(&wrapper.sender)? != deps.api.addr_validate(&owner_res.owner)? {
        return Err(ContractError::Unauthorized {});
    }

    // Check for no non-expired approvals on NFT
    // Unneeded since approvals are wiped on transfer?
    let approval_check = {
        let mut x = owner_res.approvals.clone();
        x.retain(|approval| 
            // return false (remove) approvals that are expired
            match approval.expires {
                Expiration::AtHeight(intg) => {
                    // if expired false, if not expired true
                    intg > env.block.height
                },
                Expiration::AtTime(stamp) => {
                    // if expired false, if not expired true
                    stamp > env.block.time
                },
                Expiration::Never {} => {
                    // not expired so true
                    true
                },
            }
        );
        // if x.len > 0, then x contains an unexpired approval so return error
        if x.len() > 0 {
            Err(ContractError::Unauthorized {})
        } else {
            Ok(())
        }
    };

    approval_check?;

    let msg: ReceiveNftMsg = from_binary(&wrapper.msg)?;
    
    let user_wallet = &deps.api.addr_validate(&wrapper.sender)?;

    let incoming_nft: Nft = Nft {
        contract_address: info.sender,
        token_id: wrapper.token_id,
    };

    match msg {

        ReceiveNftMsg::CreateListingCw721 { create_msg } => execute_create_listing_cw721(deps, user_wallet, incoming_nft, create_msg),

        ReceiveNftMsg::AddToListingCw721 { listing_id } => execute_add_to_sale_cw721(deps, user_wallet, incoming_nft, listing_id),

        ReceiveNftMsg::CreateBucketCw721 { bucket_id } => execute_create_bucket_cw721(deps, user_wallet, incoming_nft, bucket_id),

        ReceiveNftMsg::AddToBucketCw721 { bucket_id } => execute_add_to_bucket_cw721(deps, user_wallet, incoming_nft, bucket_id),

    }
}


//////~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
///////////~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
////////////// Query
///////////~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        // Get admin<shocking>
        QueryMsg::GetAdmin {} => to_binary(&get_admin(deps)?),
        // Get Config
        QueryMsg::GetConfig {} => to_binary(&get_config(deps)?),
        // Get a specific listing info
        QueryMsg::GetListingInfo {listing_id} => to_binary(&get_listing_info(deps, listing_id)?),
        // Get listings by owner
        QueryMsg::GetListingsByOwner {owner} => to_binary(&get_listings_by_owner(deps, owner)?),
        // Get all listings (take 100)
        QueryMsg::GetAllListings {} => to_binary(&get_all_listings(deps)?),
        // Get buckets owned by 1 address
        QueryMsg::GetBuckets {bucket_owner} => to_binary(&get_buckets(deps, bucket_owner)?),
        // Get listings finalized within 2 weeks & paginate for page
        QueryMsg::GetListingsForMarket {page_num} => to_binary(&get_listings_for_market(deps, env, page_num)?),
    }
}


//////~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
///////////~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
////////////// Tests
///////////~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[cfg(test)]
mod tests {

    //use cosmwasm_std::entry_point;
    //use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Addr};
    //use cw2::set_contract_version;
    //use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
    //use crate::state::{Config, CONFIG};
    //use crate::error::ContractError;
    //use crate::msg::AdminResponse;
    //use crate::state::{Listing};
    //use cw20::{Balance, Cw20Coin, Cw20CoinVerified, Cw20ExecuteMsg, Cw20ReceiveMsg};
    //use crate::msg::{CreateListingMsg};
    //use crate::state::*;
    //use crate::msg::*;

    #[test]
    fn test1() {
        let a = true;
        assert_eq!(a, true);
    }
}