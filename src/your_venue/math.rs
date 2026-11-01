use ethnum::I256;

use super::consts::{CURVE_SCALE, MAX_FEE_TIERS, MAX_PPM, PPM_PRECISION, PRICE_SCALE};
use crate::trading_venue::error::TradingVenueError;

const SLIPPAGE_SCALE: f64 = (MAX_PPM * PPM_PRECISION) as f64;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct FeeTier {
    pub qty: u64,
    pub more_fee_ppm: u64,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Curve {
    pub a: i64,
    pub b: i64,
    pub c: i64,
    pub target_balance: u64,
    pub bad_penalty_in_ppm: u64,
    pub bad_curve_multiplier_ppm: u64,
    pub min_slippage_in_ppm: i64,
}

pub(crate) fn overflow() -> TradingVenueError {
    TradingVenueError::CheckedMathError("arithmetic overflow".into())
}

pub(crate) fn insufficient_liquidity() -> TradingVenueError {
    TradingVenueError::AmmMethodError("insufficient liquidity".into())
}

fn slippage_exceeded() -> TradingVenueError {
    TradingVenueError::AmmMethodError("curve slippage exceeds min_slippage_in_ppm".into())
}

pub fn more_fee_from_amount_in(
    accumulated_volume: u64,
    amount_in: u64,
    fee_tiers_count: u8,
    tiers: &[FeeTier; MAX_FEE_TIERS],
    multiply_fee_amount_in_volume: u64,
) -> u64 {
    if amount_in == 0 {
        return 0;
    }
    let n = (fee_tiers_count as usize).min(MAX_FEE_TIERS);
    if n == 0 {
        return 0;
    }

    let total = accumulated_volume.saturating_add(amount_in);
    let mut weighted_sum = 0u128;

    for i in 0..n {
        let bracket_lo: u64 = if i == 0 { 0 } else { tiers[i - 1].qty };
        let bracket_hi: u64 = tiers[i].qty;

        let slice_lo = accumulated_volume.max(bracket_lo);
        let slice_hi = total.min(bracket_hi);

        if slice_hi <= slice_lo {
            continue;
        }

        let slice = (slice_hi - slice_lo) as u128;
        weighted_sum = weighted_sum.saturating_add(slice.saturating_mul(tiers[i].more_fee_ppm as u128));
    }

    let last_hi = tiers[n - 1].qty;
    if total > last_hi {
        let slice_lo = accumulated_volume.max(last_hi);
        let slice = (total - slice_lo) as u128;
        weighted_sum =
            weighted_sum.saturating_add(slice.saturating_mul(tiers[n - 1].more_fee_ppm as u128));
    }

    if accumulated_volume == 0 {
        (weighted_sum / amount_in as u128) as u64
    } else {
        let scaled =
            weighted_sum.saturating_mul(multiply_fee_amount_in_volume as u128) / amount_in as u128;
        scaled.min(u64::MAX as u128) as u64
    }
}

pub fn more_fee_rate(
    accumulated_volume: u64,
    amount_in: u64,
    fee_tiers_count: u8,
    tiers: &[FeeTier; MAX_FEE_TIERS],
    multiply_fee_amount_in_volume: u64,
) -> f64 {
    let n = (fee_tiers_count as usize).min(MAX_FEE_TIERS);
    if n == 0 {
        return 0.0;
    }
    let total = accumulated_volume.saturating_add(amount_in);
    let rate = tiers[..n]
        .iter()
        .find(|tier| total < tier.qty)
        .unwrap_or(&tiers[n - 1])
        .more_fee_ppm as f64;
    if accumulated_volume == 0 {
        rate
    } else {
        rate * multiply_fee_amount_in_volume as f64
    }
}

pub fn slot_gap_fee(slot_gap: u64, slot_gap_fee_in_ppm: u64, growth_base: u64) -> u64 {
    if slot_gap == 0 {
        return 0;
    }
    if growth_base == 0 {
        return slot_gap.saturating_mul(slot_gap_fee_in_ppm).min(MAX_PPM);
    }
    let exponent = slot_gap.saturating_sub(1).min(19) as u32;
    let multiplier = growth_base.saturating_pow(exponent);
    slot_gap_fee_in_ppm.saturating_mul(multiplier).min(MAX_PPM)
}

pub fn calculate_quote_out(
    amount_in: u64,
    token_in_decimal: u8,
    token_out_decimal: u8,
    raw_price: u64,
    fee_in_ppm: u64,
) -> Option<(u64, u64)> {
    if raw_price == 0 {
        return Some((0, 0));
    }

    let numerator = (amount_in as u128)
        .checked_mul(10u128.checked_pow(token_out_decimal as u32)?)?
        .checked_mul(raw_price as u128)?;
    let denominator = 10u128
        .checked_pow(token_in_decimal as u32)?
        .checked_mul(PRICE_SCALE as u128)?;
    let amount_out = numerator.checked_div(denominator)?;
    let fee = amount_out
        .checked_mul(fee_in_ppm as u128)?
        .checked_div(MAX_PPM as u128)?;
    let amount_out_after = amount_out.checked_sub(fee)?;
    Some((u64::try_from(amount_out_after).ok()?, u64::try_from(fee).ok()?))
}

pub fn calculate_base_out(
    amount_in: u64,
    token_in_decimal: u8,
    token_out_decimal: u8,
    raw_price: u64,
    fee_in_ppm: u64,
) -> Option<(u64, u64)> {
    if raw_price == 0 {
        return Some((0, 0));
    }
    let numerator = (amount_in as u128)
        .checked_mul(10u128.checked_pow(token_out_decimal as u32)?)?
        .checked_mul(PRICE_SCALE as u128)?;
    let denominator = 10u128
        .checked_pow(token_in_decimal as u32)?
        .checked_mul(raw_price as u128)?;
    let amount_out = numerator.checked_div(denominator)?;
    let fee = amount_out
        .checked_mul(fee_in_ppm as u128)?
        .checked_div(MAX_PPM as u128)?;
    let amount_out_after = amount_out.checked_sub(fee)?;
    Some((u64::try_from(amount_out_after).ok()?, u64::try_from(fee).ok()?))
}

pub fn raw_rate(is_base_to_quote: bool, base_decimals: u8, quote_decimals: u8, raw_price: u64) -> f64 {
    let decimals = 10f64.powi(quote_decimals as i32 - base_decimals as i32);
    if is_base_to_quote {
        raw_price as f64 * decimals / PRICE_SCALE as f64
    } else {
        PRICE_SCALE as f64 / (raw_price as f64 * decimals)
    }
}

fn eval_f(a: i64, b: i64, c: i64, balance: u64, target: u64) -> Result<i128, TradingVenueError> {
    let a = I256::from(a);
    let b = I256::from(b);
    let c = I256::from(c);
    let imb = I256::from(balance as i128 - target as i128);
    let t = I256::from(target);

    let term1 = a * imb * imb * imb / (I256::from(3) * t * t);
    let term1 = if imb < I256::ZERO { -term1 } else { term1 };
    let term2 = b * imb * imb / (I256::from(2) * t);
    let term3 = c * imb;

    i128::try_from(term1 + term2 + term3).map_err(|_| overflow())
}

fn calc_integral_internal(
    a: i64,
    b: i64,
    c: i64,
    target_balance: u64,
    start_balance: u64,
    end_balance: u64,
    clamp: bool,
) -> Result<i128, TradingVenueError> {
    if start_balance > end_balance {
        return Ok(
            -calc_integral_internal(a, b, 0, target_balance, end_balance, start_balance, clamp)?
                + c as i128 * (start_balance - end_balance) as i128,
        );
    }

    if clamp {
        let two_target = target_balance.checked_mul(2).ok_or_else(overflow)?;
        if start_balance >= two_target {
            let width = (end_balance - start_balance) as i128;
            let rate = a as i128 + b as i128 + c as i128;
            return width.checked_mul(rate).ok_or_else(overflow);
        }
        if end_balance > two_target {
            return Ok(
                calc_integral_internal(a, b, c, target_balance, start_balance, two_target, clamp)?
                    + calc_integral_internal(a, b, c, target_balance, two_target, end_balance, clamp)?,
            );
        }
    }

    if target_balance == 0 {
        return Ok(0);
    }
    let f_end = eval_f(a, b, c, end_balance, target_balance)?;
    let f_start = eval_f(a, b, c, start_balance, target_balance)?;
    f_end.checked_sub(f_start).ok_or_else(overflow)
}

fn calculate_pool_integral(
    curve: &Curve,
    start_balance: u64,
    amount: u64,
    is_base_to_quote: bool,
) -> Result<i128, TradingVenueError> {
    let Curve { a, b, c, target_balance, .. } = *curve;
    let end_balance = if is_base_to_quote {
        start_balance.checked_add(amount).ok_or_else(overflow)?
    } else {
        start_balance.checked_sub(amount).ok_or_else(insufficient_liquidity)?
    };

    let mixed = (start_balance <= target_balance && target_balance <= end_balance)
        || (end_balance <= target_balance && target_balance <= start_balance);

    let (good, bad) = if mixed {
        let g = calc_integral_internal(a, b, c, target_balance, start_balance, target_balance, true)?;
        let b_ = calc_integral_internal(a, b, c, target_balance, target_balance, end_balance, false)?;
        (g, b_)
    } else {
        let closer = end_balance.abs_diff(target_balance) <= start_balance.abs_diff(target_balance);
        let val = calc_integral_internal(a, b, c, target_balance, start_balance, end_balance, closer)?;
        if closer { (val, 0i128) } else { (0i128, val) }
    };

    let scaled_bad = bad * curve.bad_curve_multiplier_ppm as i128 / MAX_PPM as i128;
    let penalized_bad =
        scaled_bad * (MAX_PPM as i128 + curve.bad_penalty_in_ppm as i128) / MAX_PPM as i128;
    Ok(good + penalized_bad)
}

pub fn adjust_to_balance(
    curve: &Curve,
    pool_base_reserve: u64,
    is_base_to_quote: bool,
    in_amount: u64,
    out_amount: u64,
) -> Result<(u64, i128), TradingVenueError> {
    let amount = if is_base_to_quote { in_amount } else { out_amount };
    if amount == 0 || out_amount == 0 {
        return Ok((0, 0));
    }

    let integral = calculate_pool_integral(curve, pool_base_reserve, amount, is_base_to_quote)?;

    let slippage_ppm = (MAX_PPM as i128).checked_mul(integral).ok_or_else(overflow)?
        / (amount as i128 * CURVE_SCALE as i128);

    if slippage_ppm <= curve.min_slippage_in_ppm as i128 * PPM_PRECISION as i128 {
        return Err(slippage_exceeded());
    }

    let adjusted = out_amount as i128 * (MAX_PPM as i128 * PPM_PRECISION as i128 - slippage_ppm)
        / (MAX_PPM as i128 * PPM_PRECISION as i128);

    let adjusted = u64::try_from(adjusted).map_err(|_| slippage_exceeded())?;
    Ok((adjusted, integral))
}

fn curve_rate(curve: &Curve, end_balance: f64, is_base_to_quote: bool) -> f64 {
    let t = curve.target_balance as f64;
    if t == 0.0 {
        return 0.0;
    }
    let (a, b, c) = (curve.a as f64, curve.b as f64, curve.c as f64);

    let bad = if is_base_to_quote { end_balance > t } else { end_balance < t };
    let imb = if !bad && end_balance >= 2.0 * t { t } else { end_balance - t };
    let y_ab = a * imb * imb.abs() / (t * t) + b * imb / t;

    let rate = if is_base_to_quote { y_ab + c } else { c - y_ab };
    if bad {
        rate * curve.bad_curve_multiplier_ppm as f64 / MAX_PPM as f64
            * (MAX_PPM + curve.bad_penalty_in_ppm) as f64
            / MAX_PPM as f64
    } else {
        rate
    }
}

#[allow(clippy::too_many_arguments)]
pub fn marginal_price(
    curve: &Curve,
    pool_base_reserve: u64,
    is_base_to_quote: bool,
    rate: f64,
    marginal_fee_ppm: f64,
    amount_in: u64,
    out_before_curve: u64,
    integral: i128,
) -> f64 {
    let du = rate * (1.0 - marginal_fee_ppm / MAX_PPM as f64);
    let reserve = pool_base_reserve as f64;

    if is_base_to_quote {
        let end = reserve + amount_in as f64;
        let rate_end = curve_rate(curve, end, true) / CURVE_SCALE as f64 * MAX_PPM as f64;
        if amount_in == 0 {
            return du * (1.0 - rate_end / SLIPPAGE_SCALE);
        }
        let x = amount_in as f64;
        let avg = integral as f64 / CURVE_SCALE as f64 * MAX_PPM as f64 / x;
        du * (1.0 - avg / SLIPPAGE_SCALE)
            - out_before_curve as f64 * (rate_end - avg) / (x * SLIPPAGE_SCALE)
    } else {
        let end = reserve - out_before_curve as f64;
        let rate_end = curve_rate(curve, end, false) / CURVE_SCALE as f64 * MAX_PPM as f64;
        du * (1.0 - rate_end / SLIPPAGE_SCALE)
    }
}
