//! Helpers for implementing formatting for ISO 8601.

use core::cmp::min;
use std::io;

use deranged::{ru8, ru16, ru32};
use num_conv::prelude::*;

use crate::error;
use crate::format_description::modifier::Padding;
use crate::format_description::well_known::Iso8601;
use crate::format_description::well_known::iso8601::{
    DateKind, EncodedConfig, OffsetPrecision, TimePrecision,
};
use crate::formatting::{ComponentProvider, Output};
use crate::internal_macros::try_likely_ok;
use crate::unit::*;

impl<W> Output<W>
where
    W: io::Write + ?Sized,
{
    /// Format the date portion of ISO 8601.
    pub(super) fn format_iso8601_date<V, const CONFIG: EncodedConfig>(
        &mut self,
        value: &V,
        state: &mut V::State,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        match Iso8601::<CONFIG>::DATE_KIND {
            DateKind::Calendar => {
                let year = value.calendar_year(state).get();

                if Iso8601::<CONFIG>::YEAR_IS_SIX_DIGITS {
                    try_likely_ok!(self.write_if_else(year < 0, "-", "+"));
                    // Safety: `calendar_year` returns a value whose absolute value is guaranteed to
                    // be less than 1,000,000.
                    try_likely_ok!(self.format_six_digits_pad_zero(unsafe {
                        ru32::new_unchecked(year.unsigned_abs())
                    }));
                } else {
                    let year = try_likely_ok!(
                        ru16::new(year.cast_unsigned().truncate())
                            .ok_or(error::Format::InvalidComponent("year"))
                    );
                    try_likely_ok!(self.format_four_digits_pad_zero(year));
                }
                try_likely_ok!(self.write_if(Iso8601::<CONFIG>::USE_SEPARATORS, "-"));
                try_likely_ok!(self.format_two_digits(
                    // Safety: `month` is guaranteed to be in the range `1..=12`.
                    unsafe { ru8::new_unchecked(u8::from(value.month(state))) },
                    Padding::Zero,
                ));
                try_likely_ok!(self.write_if(Iso8601::<CONFIG>::USE_SEPARATORS, "-"));
                try_likely_ok!(self.format_two_digits(value.day(state).expand(), Padding::Zero));
            }
            DateKind::Week => {
                let year = value.iso_year(state).get();

                if Iso8601::<CONFIG>::YEAR_IS_SIX_DIGITS {
                    try_likely_ok!(self.write_if_else(year < 0, "-", "+"));
                    // Safety: `iso_year` returns a value whose absolute value is guaranteed to be
                    // less than 1,000,000.
                    try_likely_ok!(self.format_six_digits_pad_zero(unsafe {
                        ru32::new_unchecked(year.unsigned_abs())
                    }));
                } else {
                    let year = try_likely_ok!(
                        ru16::new(year.cast_unsigned().truncate())
                            .ok_or(error::Format::InvalidComponent("year"))
                    );
                    try_likely_ok!(self.format_four_digits_pad_zero(year));
                }
                try_likely_ok!(self.write_if_else(Iso8601::<CONFIG>::USE_SEPARATORS, "-W", "W"));
                try_likely_ok!(
                    self.format_two_digits(value.iso_week_number(state).expand(), Padding::Zero)
                );
                try_likely_ok!(self.write_if(Iso8601::<CONFIG>::USE_SEPARATORS, "-"));
                // Safety: The value is in the range `1..=7`.
                try_likely_ok!(self.format_single_digit(unsafe {
                    ru8::new_unchecked(value.weekday(state).number_from_monday())
                }));
            }
            DateKind::Ordinal => {
                let year = value.calendar_year(state).get();

                if Iso8601::<CONFIG>::YEAR_IS_SIX_DIGITS {
                    try_likely_ok!(self.write_if_else(year < 0, "-", "+"));
                    // Safety: `calendar_year` returns a value whose absolute value is guaranteed to
                    // be less than 1,000,000.
                    try_likely_ok!(self.format_six_digits_pad_zero(unsafe {
                        ru32::new_unchecked(year.unsigned_abs())
                    }));
                } else {
                    let year = try_likely_ok!(
                        ru16::new(year.cast_unsigned().truncate())
                            .ok_or(error::Format::InvalidComponent("year"))
                    );
                    try_likely_ok!(self.format_four_digits_pad_zero(year));
                }
                try_likely_ok!(self.write_if(Iso8601::<CONFIG>::USE_SEPARATORS, "-"));
                try_likely_ok!(
                    self.format_three_digits(value.ordinal(state).expand(), Padding::Zero)
                );
            }
        }

        Ok(())
    }

    /// Format the time portion of ISO 8601.
    #[inline]
    pub(super) fn format_iso8601_time<V, const CONFIG: EncodedConfig>(
        &mut self,
        value: &V,
        state: &mut V::State,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        // The "T" can only be omitted in extended format where there is no date being formatted.
        try_likely_ok!(self.write_if(
            !Iso8601::<CONFIG>::USE_SEPARATORS || Iso8601::<CONFIG>::FORMAT_DATE,
            "T",
        ));

        match Iso8601::<CONFIG>::TIME_PRECISION {
            TimePrecision::Hour { decimal_digits } => {
                let hours = (value.hour(state).get() as f64)
                    + (value.minute(state).get() as f64) / Minute::per_t::<f64>(Hour)
                    + (value.second(state).get() as f64) / Second::per_t::<f64>(Hour)
                    + (value.nanosecond(state).get() as f64) / Nanosecond::per_t::<f64>(Hour);
                try_likely_ok!(self.format_float(hours, 2, decimal_digits));
            }
            TimePrecision::Minute { decimal_digits } => {
                try_likely_ok!(self.format_two_digits(value.hour(state).expand(), Padding::Zero));
                try_likely_ok!(self.write_if(Iso8601::<CONFIG>::USE_SEPARATORS, ":"));
                let minutes = (value.minute(state).get() as f64)
                    + (value.second(state).get() as f64) / Second::per_t::<f64>(Minute)
                    + (value.nanosecond(state).get() as f64) / Nanosecond::per_t::<f64>(Minute);
                try_likely_ok!(self.format_float(minutes, 2, decimal_digits));
            }
            TimePrecision::Second { decimal_digits } => {
                try_likely_ok!(self.format_two_digits(value.hour(state).expand(), Padding::Zero));
                try_likely_ok!(self.write_if(Iso8601::<CONFIG>::USE_SEPARATORS, ":"));
                try_likely_ok!(self.format_two_digits(value.minute(state).expand(), Padding::Zero));
                try_likely_ok!(self.write_if(Iso8601::<CONFIG>::USE_SEPARATORS, ":"));
                try_likely_ok!(self.format_two_digits(value.second(state).expand(), Padding::Zero));
                if let Some(digits) = decimal_digits {
                    const POW_TABLE: [u64; 9] = [
                        1,
                        10,
                        100,
                        1_000,
                        10_000,
                        100_000,
                        1_000_000,
                        10_000_000,
                        100_000_000,
                    ];

                    try_likely_ok!(self.write("."));
                    let nano = value.nanosecond(state).get() as u64;
                    let sub_digits = min(digits.get(), 9);
                    let truncated = nano / POW_TABLE[9 - sub_digits as usize];
                    try_likely_ok!(self.format_int_padded(truncated, sub_digits));
                    for _ in 9..digits.get() {
                        try_likely_ok!(self.write("0"));
                    }
                }
            }
        }

        Ok(())
    }

    /// Format the UTC offset portion of ISO 8601.
    #[inline]
    pub(super) fn format_iso8601_offset<V, const CONFIG: EncodedConfig>(
        &mut self,
        value: &V,
        state: &mut V::State,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        if Iso8601::<CONFIG>::FORMAT_TIME && value.offset_is_utc(state) {
            return self.write("Z").map_err(Into::into);
        }

        if value.offset_second(state).get() != 0 {
            return Err(error::Format::InvalidComponent("offset_second"));
        }
        try_likely_ok!(self.write_if_else(value.offset_is_negative(state), "-", "+"));
        try_likely_ok!(self.format_two_digits(
            // Safety: The value is in the range `-25..=25`.
            unsafe { ru8::new_unchecked(value.offset_hour(state).get().unsigned_abs()) },
            Padding::Zero,
        ));

        let minutes = value.offset_minute(state);

        if Iso8601::<CONFIG>::OFFSET_PRECISION == OffsetPrecision::Hour && minutes.get() != 0 {
            return Err(error::Format::InvalidComponent("offset_minute"));
        } else if Iso8601::<CONFIG>::OFFSET_PRECISION == OffsetPrecision::Minute {
            try_likely_ok!(self.write_if(Iso8601::<CONFIG>::USE_SEPARATORS, ":"));
            try_likely_ok!(self.format_two_digits(
                // Safety: The value is in the range `0..=59`.
                unsafe { ru8::new_unchecked(minutes.get().unsigned_abs()) },
                Padding::Zero,
            ));
        }

        Ok(())
    }
}
