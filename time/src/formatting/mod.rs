//! Formatting for various types.

mod component_provider;
pub(crate) mod formattable;
mod iso8601;
mod metadata;

use core::mem::MaybeUninit;
use core::num::NonZero;
use std::io;

use deranged::{Option_ri32, Option_ru8, ri8, ri16, ri32, ru8, ru16, ru32};
use num_conv::prelude::*;

use self::component_provider::ComponentProvider;
pub use self::formattable::Formattable;
use crate::format_description::{Period, modifier};
use crate::internal_macros::try_likely_ok;
use crate::time::{Hours, Minutes, Nanoseconds, Seconds};
use crate::utc_offset::{Hours as OffsetHours, Minutes as OffsetMinutes, Seconds as OffsetSeconds};
use crate::{Month, Weekday, error, num_fmt};

type Day = ru8<1, 31>;
type OptionDay = Option_ru8<1, 31>;
type Ordinal = ru16<1, 366>;
type IsoWeekNumber = ru8<1, 53>;
type OptionIsoWeekNumber = Option_ru8<1, 53>;
type MondayBasedWeek = ru8<0, 53>;
type SundayBasedWeek = ru8<0, 53>;
type Year = ri32<-999_999, 999_999>;
type StandardYear = ri16<-9_999, 9_999>;
type OptionYear = Option_ri32<-999_999, 999_999>;
type ExtendedCentury = ri16<-9_999, 9_999>;
type StandardCentury = ri8<-99, 99>;
type LastTwo = ru8<0, 99>;

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const WEEKDAY_NAMES: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// Helper function to obtain 10^x, guaranteeing determinism for x ≤ 9. For these cases, the
/// function optimizes to a lookup table. For x ≥ 10, it falls back to `10_f64.powi(x)`. The only
/// situation where this would occur is if the user explicitly requests such precision when
/// configuring the ISO 8601 well known format. All other possibilities max out at nine digits.
#[inline]
fn f64_10_pow_x(x: NonZero<u8>) -> f64 {
    match x.get() {
        1 => 10.,
        2 => 100.,
        3 => 1_000.,
        4 => 10_000.,
        5 => 100_000.,
        6 => 1_000_000.,
        7 => 10_000_000.,
        8 => 100_000_000.,
        9 => 1_000_000_000.,
        x => 10_f64.powi(x.cast_signed().widen()),
    }
}

/// An `io::Write`r that keeps track of the number of bytes written to it.
#[derive(Debug)]
pub(crate) struct Output<W>
where
    W: ?Sized,
{
    /// The number of bytes written to the output.
    pub(crate) bytes_written: usize,
    /// The output that bytes are written to.
    pub(crate) output: W,
}

impl<W> Output<W>
where
    W: io::Write + ?Sized,
{
    /// Write all bytes to the output.
    #[inline]
    pub(crate) fn write(&mut self, s: &str) -> io::Result<()> {
        try_likely_ok!(self.output.write_all(s.as_bytes()));
        self.bytes_written += s.len();
        Ok(())
    }

    /// Write the string to the output.
    #[inline]
    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        try_likely_ok!(self.output.write_all(bytes));
        self.bytes_written += bytes.len();
        Ok(())
    }

    /// Write all strings to the output (in order).
    #[inline]
    pub(crate) fn write_many<const N: usize>(&mut self, arr: [&str; N]) -> io::Result<()> {
        for s in arr {
            try_likely_ok!(self.write(s));
        }
        Ok(())
    }

    /// Write the string to the output if and only if `pred` is true.
    pub(crate) fn write_if(&mut self, pred: bool, s: &str) -> io::Result<()> {
        if pred { self.write(s) } else { Ok(()) }
    }

    /// If and only if `pred` is true, write `true_str` to the output. Otherwise, write `false_str`.
    #[inline]
    pub(crate) fn write_if_else(
        &mut self,
        pred: bool,
        true_str: &str,
        false_str: &str,
    ) -> io::Result<()> {
        self.write(if pred { true_str } else { false_str })
    }

    /// Write an integer with zeros as trailing padding if necessary to reach the requested width.
    ///
    /// This function is intended to be used for formatting the fractional part of a value, as the
    /// trailing zeros would change the semantic meaning for non-fractional values.
    #[inline]
    fn format_int_padded(&mut self, value: u64, width: u8) -> io::Result<()> {
        let s = num_fmt::u64_pad_none(value);
        let digit_count = s.len() as u8;
        for _ in digit_count..width {
            try_likely_ok!(self.write("0"));
        }
        try_likely_ok!(self.write(&s));
        Ok(())
    }

    /// Write the floating point number to the output.
    ///
    /// This method accepts the number of digits before and after the decimal. The value will be
    /// padded with zeroes to the left if necessary.
    #[inline]
    pub(crate) fn format_float(
        &mut self,
        mut value: f64,
        digits_before_decimal: u8,
        digits_after_decimal: Option<NonZero<u8>>,
    ) -> io::Result<()> {
        match digits_after_decimal {
            Some(digits_after_decimal) => {
                // If the precision is less than nine digits after the decimal point, truncate the
                // value. This avoids rounding up and causing the value to exceed the maximum
                // permitted value (as in #678). If the precision is at least nine, then we don't
                // truncate so as to avoid having an off-by-one error (as in #724). The latter is
                // necessary because floating point values are inherently imprecise with decimal
                // values, so a minuscule error can be amplified easily.
                //
                // Note that this is largely an issue for second values, as for minute and hour
                // decimals the value is divided by 60 or 3,600, neither of which divide evenly into
                // 10^x.
                //
                // While not a perfect approach, this addresses the bugs that have been reported so
                // far without being overly complex.
                if digits_after_decimal.get() < 9 {
                    let trunc_num = f64_10_pow_x(digits_after_decimal);
                    value = f64::trunc(value * trunc_num) / trunc_num;

                    let int_part = value.trunc() as u64;
                    let frac_part =
                        f64::round(value.fract() * f64_10_pow_x(digits_after_decimal)) as u64;

                    try_likely_ok!(self.format_int_padded(int_part, digits_before_decimal.widen()));
                    try_likely_ok!(self.write("."));
                    try_likely_ok!(
                        self.format_int_padded(frac_part, digits_after_decimal.get().widen())
                    );
                } else {
                    // For precision >= 9, use write! to avoid off-by-one errors from floating point
                    // rounding (see #724). Integer extraction of the fractional part could overflow
                    // the digit count when rounding causes a carry.
                    let digits_after = digits_after_decimal.get().widen::<usize>();
                    let width = digits_before_decimal.widen::<usize>() + 1 + digits_after;
                    try_likely_ok!(write!(self.output, "{value:0>width$.digits_after$}"));
                    self.bytes_written += width;
                }
                Ok(())
            }
            None => self.format_int_padded(value.trunc() as u64, digits_before_decimal),
        }
    }

    /// Format a single digit.
    #[inline]
    pub(crate) fn format_single_digit(&mut self, value: ru8<0, 9>) -> io::Result<()> {
        self.write(num_fmt::single_digit(value))
    }

    /// Format a two digit number with the specified padding.
    #[inline]
    pub(crate) fn format_two_digits(
        &mut self,
        value: ru8<0, 99>,
        padding: modifier::Padding,
    ) -> io::Result<()> {
        let s = match padding {
            modifier::Padding::Space => num_fmt::two_digits_space_padded(value),
            modifier::Padding::Zero => num_fmt::two_digits_zero_padded(value),
            modifier::Padding::None => num_fmt::one_to_two_digits_no_padding(value),
        };
        self.write(s)
    }

    /// Format a three digit number with the specified padding.
    #[inline]
    pub(crate) fn format_three_digits(
        &mut self,
        value: ru16<0, 999>,
        padding: modifier::Padding,
    ) -> io::Result<()> {
        let [first, second_and_third] = match padding {
            modifier::Padding::Space => num_fmt::three_digits_space_padded(value),
            modifier::Padding::Zero => num_fmt::three_digits_zero_padded(value),
            modifier::Padding::None => num_fmt::one_to_three_digits_no_padding(value),
        };
        self.write_many([first, second_and_third])
    }

    /// Format a four digit number with the specified padding.
    #[inline]
    pub(crate) fn format_four_digits(
        &mut self,
        value: ru16<0, 9_999>,
        padding: modifier::Padding,
    ) -> io::Result<()> {
        let [first_and_second, third_and_fourth] = match padding {
            modifier::Padding::Space => num_fmt::four_digits_space_padded(value),
            modifier::Padding::Zero => num_fmt::four_digits_zero_padded(value),
            modifier::Padding::None => num_fmt::one_to_four_digits_no_padding(value),
        };
        self.write_many([first_and_second, third_and_fourth])
    }

    /// Format a four digit number that is padded with zeroes.
    #[inline]
    pub(crate) fn format_four_digits_pad_zero(&mut self, value: ru16<0, 9_999>) -> io::Result<()> {
        self.write_many(num_fmt::four_digits_zero_padded(value))
    }

    /// Format a five digit number that is padded with zeroes.
    #[inline]
    pub(crate) fn format_five_digits_pad_zero(&mut self, value: ru32<0, 99_999>) -> io::Result<()> {
        self.write_many(num_fmt::five_digits_zero_padded(value))
    }

    /// Format a six digit number that is padded with zeroes.
    #[inline]
    pub(crate) fn format_six_digits_pad_zero(&mut self, value: ru32<0, 999_999>) -> io::Result<()> {
        self.write_many(num_fmt::six_digits_zero_padded(value))
    }

    /// Format a number with no padding.
    ///
    /// If the sign is mandatory, the sign must be written by the caller.
    #[inline]
    pub(crate) fn format_u64_pad_none(&mut self, value: u64) -> io::Result<()> {
        self.write(&num_fmt::u64_pad_none(value))
    }

    /// Format a number with no padding.
    ///
    /// If the sign is mandatory, the sign must be written by the caller.
    #[inline]
    pub(crate) fn format_u128_pad_none(&mut self, value: u128) -> io::Result<()> {
        self.write(&num_fmt::u128_pad_none(value))
    }

    /// Format the day into the designated output.
    #[inline]
    fn fmt_day(
        &mut self,
        day: Day,
        modifier::Day { padding }: modifier::Day,
    ) -> Result<(), io::Error> {
        self.format_two_digits(day.expand(), padding)
    }

    /// Format the month into the designated output using the abbreviated name.
    #[inline]
    fn fmt_month_short(
        &mut self,
        month: Month,
        modifier::MonthShort {
            case_sensitive: _, // no effect on formatting
        }: modifier::MonthShort,
    ) -> io::Result<()> {
        // Safety: All month names are at least three bytes long.
        self.write(unsafe { MONTH_NAMES[u8::from(month).widen::<usize>() - 1].get_unchecked(..3) })
    }

    /// Format the month into the designated output using the full name.
    #[inline]
    fn fmt_month_long(
        &mut self,
        month: Month,
        modifier::MonthLong {
            case_sensitive: _, // no effect on formatting
        }: modifier::MonthLong,
    ) -> io::Result<()> {
        self.write(MONTH_NAMES[u8::from(month).widen::<usize>() - 1])
    }

    /// Format the month into the designated output as a number from 1-12.
    #[inline]
    fn fmt_month_numerical(
        &mut self,
        month: Month,
        modifier::MonthNumerical { padding }: modifier::MonthNumerical,
    ) -> io::Result<()> {
        // Safety: The month is guaranteed to be in the range `1..=12`.
        self.format_two_digits(unsafe { ru8::new_unchecked(u8::from(month)) }, padding)
    }

    /// Format the ordinal into the designated output.
    #[inline]
    fn fmt_ordinal(
        &mut self,
        ordinal: Ordinal,
        modifier::Ordinal { padding }: modifier::Ordinal,
    ) -> Result<(), io::Error> {
        self.format_three_digits(ordinal.expand(), padding)
    }

    /// Format the weekday into the designated output using the abbreviated name.
    #[inline]
    fn fmt_weekday_short(
        &mut self,
        weekday: Weekday,
        modifier::WeekdayShort {
            case_sensitive: _, // no effect on formatting
        }: modifier::WeekdayShort,
    ) -> io::Result<()> {
        // Safety: All weekday names are at least three bytes long.
        self.write(unsafe {
            WEEKDAY_NAMES[weekday.number_days_from_monday().widen::<usize>()].get_unchecked(..3)
        })
    }

    /// Format the weekday into the designated output using the full name.
    #[inline]
    fn fmt_weekday_long(
        &mut self,
        weekday: Weekday,
        modifier::WeekdayLong {
            case_sensitive: _, // no effect on formatting
        }: modifier::WeekdayLong,
    ) -> io::Result<()> {
        self.write(WEEKDAY_NAMES[weekday.number_days_from_monday().widen::<usize>()])
    }

    /// Format the weekday into the designated output as a number from either 0-6 or 1-7 (depending
    /// on the modifier), where Sunday is either 0 or 1.
    #[inline]
    fn fmt_weekday_sunday(
        &mut self,
        weekday: Weekday,
        modifier::WeekdaySunday { one_indexed }: modifier::WeekdaySunday,
    ) -> io::Result<()> {
        // Safety: The value is guaranteed to be in the range `0..=7`.
        self.format_single_digit(unsafe {
            ru8::new_unchecked(weekday.number_days_from_sunday() + u8::from(one_indexed))
        })
    }

    /// Format the weekday into the designated output as a number from either 0-6 or 1-7 (depending
    /// on the modifier), where Monday is either 0 or 1.
    #[inline]
    fn fmt_weekday_monday(
        &mut self,
        weekday: Weekday,
        modifier::WeekdayMonday { one_indexed }: modifier::WeekdayMonday,
    ) -> io::Result<()> {
        // Safety: The value is guaranteed to be in the range `0..=7`.
        self.format_single_digit(unsafe {
            ru8::new_unchecked(weekday.number_days_from_monday() + u8::from(one_indexed))
        })
    }

    #[inline]
    fn fmt_week_number_iso(
        &mut self,
        week_number: IsoWeekNumber,
        modifier::WeekNumberIso { padding }: modifier::WeekNumberIso,
    ) -> io::Result<()> {
        self.format_two_digits(week_number.expand(), padding)
    }

    #[inline]
    fn fmt_week_number_sunday(
        &mut self,
        week_number: SundayBasedWeek,
        modifier::WeekNumberSunday { padding }: modifier::WeekNumberSunday,
    ) -> io::Result<()> {
        self.format_two_digits(week_number.expand(), padding)
    }

    #[inline]
    fn fmt_week_number_monday(
        &mut self,
        week_number: MondayBasedWeek,
        modifier::WeekNumberMonday { padding }: modifier::WeekNumberMonday,
    ) -> io::Result<()> {
        self.format_two_digits(week_number.expand(), padding)
    }

    #[inline]
    fn fmt_calendar_year_full_extended_range(
        &mut self,
        full_year: Year,
        modifier::CalendarYearFullExtendedRange {
            padding,
            sign_is_mandatory,
        }: modifier::CalendarYearFullExtendedRange,
    ) -> io::Result<()> {
        try_likely_ok!(self.fmt_sign(
            full_year.is_negative(),
            sign_is_mandatory || full_year.get() >= 10_000
        ));
        // Safety: We just called `.abs()`, so zero is the minimum. The maximum is
        // unchanged.
        let value: ru32<0, 999_999> =
            unsafe { full_year.abs().narrow_unchecked::<0, 999_999>().into() };

        if let Some(value) = value.narrow::<0, 9_999>() {
            try_likely_ok!(self.format_four_digits(value.into(), padding))
        } else if let Some(value) = value.narrow::<0, 99_999>() {
            try_likely_ok!(self.format_five_digits_pad_zero(value))
        } else {
            try_likely_ok!(self.format_six_digits_pad_zero(value))
        };
        Ok(())
    }

    #[inline]
    fn fmt_calendar_year_full_standard_range(
        &mut self,
        full_year: StandardYear,
        modifier::CalendarYearFullStandardRange {
            padding,
            sign_is_mandatory,
        }: modifier::CalendarYearFullStandardRange,
    ) -> io::Result<()> {
        try_likely_ok!(self.fmt_sign(full_year.is_negative(), sign_is_mandatory));
        try_likely_ok!(self.format_four_digits(
            // Safety: The minimum is zero due to the `.abs()` call; the maximum is unchanged.
            unsafe { full_year.abs().narrow_unchecked::<0, 9_999>().into() },
            padding
        ));
        Ok(())
    }

    #[inline]
    fn fmt_iso_year_full_extended_range(
        &mut self,
        full_year: Year,
        modifier::IsoYearFullExtendedRange {
            padding,
            sign_is_mandatory,
        }: modifier::IsoYearFullExtendedRange,
    ) -> io::Result<()> {
        try_likely_ok!(self.fmt_sign(
            full_year.is_negative(),
            sign_is_mandatory || full_year.get() >= 10_000
        ));
        // Safety: The minimum is zero due to the `.abs()` call, with the maximum is unchanged.
        let value: ru32<0, 999_999> =
            unsafe { full_year.abs().narrow_unchecked::<0, 999_999>().into() };

        if let Some(value) = value.narrow::<0, 9_999>() {
            try_likely_ok!(self.format_four_digits(value.into(), padding))
        } else if let Some(value) = value.narrow::<0, 99_999>() {
            try_likely_ok!(self.format_five_digits_pad_zero(value))
        } else {
            try_likely_ok!(self.format_six_digits_pad_zero(value))
        };
        Ok(())
    }

    #[inline]
    fn fmt_iso_year_full_standard_range(
        &mut self,
        year: StandardYear,
        modifier::IsoYearFullStandardRange {
            padding,
            sign_is_mandatory,
        }: modifier::IsoYearFullStandardRange,
    ) -> io::Result<()> {
        try_likely_ok!(self.fmt_sign(year.is_negative(), sign_is_mandatory));
        try_likely_ok!(self.format_four_digits(
            // Safety: The minimum is zero due to the `.abs()` call; the maximum is unchanged.
            unsafe { year.abs().narrow_unchecked::<0, 9_999>().into() },
            padding
        ));
        Ok(())
    }

    #[inline]
    fn fmt_calendar_year_century_extended_range(
        &mut self,
        century: ExtendedCentury,
        is_negative: bool,
        modifier::CalendarYearCenturyExtendedRange {
            padding,
            sign_is_mandatory,
        }: modifier::CalendarYearCenturyExtendedRange,
    ) -> io::Result<()> {
        try_likely_ok!(self.fmt_sign(is_negative, sign_is_mandatory || century.get() >= 100));
        // Safety: The minimum is zero due to the `.abs()` call;  the maximum is unchanged.
        let century: ru16<0, 9_999> =
            unsafe { century.abs().narrow_unchecked::<0, 9_999>().into() };

        if let Some(century) = century.narrow::<0, 99>() {
            try_likely_ok!(self.format_two_digits(century.into(), padding))
        } else if let Some(century) = century.narrow::<0, 999>() {
            try_likely_ok!(self.format_three_digits(century, padding))
        } else {
            try_likely_ok!(self.format_four_digits(century, padding))
        };
        Ok(())
    }

    #[inline]
    fn fmt_calendar_year_century_standard_range(
        &mut self,
        century: StandardCentury,
        is_negative: bool,
        modifier::CalendarYearCenturyStandardRange {
            padding,
            sign_is_mandatory,
        }: modifier::CalendarYearCenturyStandardRange,
    ) -> io::Result<()> {
        try_likely_ok!(self.fmt_sign(is_negative, sign_is_mandatory));
        // Safety: The minimum is zero due to the `.unsigned_abs()` call.
        let century = unsafe { century.abs().narrow_unchecked::<0, 99>() };
        try_likely_ok!(self.format_two_digits(century.into(), padding));
        Ok(())
    }

    #[inline]
    fn fmt_iso_year_century_extended_range(
        &mut self,
        century: ExtendedCentury,
        is_negative: bool,
        modifier::IsoYearCenturyExtendedRange {
            padding,
            sign_is_mandatory,
        }: modifier::IsoYearCenturyExtendedRange,
    ) -> io::Result<()> {
        try_likely_ok!(self.fmt_sign(is_negative, sign_is_mandatory || century.get() >= 100));
        // Safety: The minimum is zero due to the `.unsigned_abs()` call, with the maximum is
        // unchanged.
        let century: ru16<0, 9_999> =
            unsafe { century.abs().narrow_unchecked::<0, 9_999>().into() };

        if let Some(century) = century.narrow::<0, 99>() {
            try_likely_ok!(self.format_two_digits(century.into(), padding))
        } else if let Some(century) = century.narrow::<0, 999>() {
            try_likely_ok!(self.format_three_digits(century, padding))
        } else {
            try_likely_ok!(self.format_four_digits(century, padding))
        };
        Ok(())
    }

    #[inline]
    fn fmt_iso_year_century_standard_range(
        &mut self,
        century: StandardCentury,
        is_negative: bool,
        modifier::IsoYearCenturyStandardRange {
            padding,
            sign_is_mandatory,
        }: modifier::IsoYearCenturyStandardRange,
    ) -> io::Result<()> {
        try_likely_ok!(self.fmt_sign(is_negative, sign_is_mandatory));
        // Safety: The minimum is zero due to the `.unsigned_abs()` call.
        let century = unsafe { century.abs().narrow_unchecked::<0, 99>() };
        try_likely_ok!(self.format_two_digits(century.into(), padding));
        Ok(())
    }

    #[inline]
    fn fmt_calendar_year_last_two(
        &mut self,
        last_two: LastTwo,
        modifier::CalendarYearLastTwo { padding }: modifier::CalendarYearLastTwo,
    ) -> io::Result<()> {
        self.format_two_digits(last_two, padding)
    }

    #[inline]
    fn fmt_iso_year_last_two(
        &mut self,
        last_two: LastTwo,
        modifier::IsoYearLastTwo { padding }: modifier::IsoYearLastTwo,
    ) -> io::Result<()> {
        self.format_two_digits(last_two, padding)
    }

    /// Format the hour into the designated output using the 12-hour clock.
    #[inline]
    fn fmt_hour_12(
        &mut self,
        hour: Hours,
        modifier::Hour12 { padding }: modifier::Hour12,
    ) -> io::Result<()> {
        // Safety: The value is guaranteed to be in the range `1..=12`.
        self.format_two_digits(
            unsafe { ru8::new_unchecked((hour.get() + 11) % 12 + 1) },
            padding,
        )
    }

    /// Format the hour into the designated output using the 24-hour clock.
    #[inline]
    fn fmt_hour_24(
        &mut self,
        hour: Hours,
        modifier::Hour24 { padding }: modifier::Hour24,
    ) -> io::Result<()> {
        self.format_two_digits(hour.expand(), padding)
    }

    /// Format the minute into the designated output.
    #[inline]
    fn fmt_minute(
        &mut self,
        minute: Minutes,
        modifier::Minute { padding }: modifier::Minute,
    ) -> Result<(), io::Error> {
        self.format_two_digits(minute.expand(), padding)
    }

    /// Format the period into the designated output.
    #[inline]
    fn fmt_period(
        &mut self,
        period: Period,
        modifier::Period {
            is_uppercase,
            case_sensitive: _, // no effect on formatting
        }: modifier::Period,
    ) -> Result<(), io::Error> {
        self.write(match (period, is_uppercase) {
            (Period::Am, false) => "am",
            (Period::Am, true) => "AM",
            (Period::Pm, false) => "pm",
            (Period::Pm, true) => "PM",
        })
    }

    /// Format the second into the designated output.
    #[inline]
    fn fmt_second(
        &mut self,
        second: Seconds,
        modifier::Second { padding }: modifier::Second,
    ) -> Result<(), io::Error> {
        self.format_two_digits(second.expand(), padding)
    }

    /// Format the subsecond into the designated output.
    #[inline]
    fn fmt_subsecond(
        &mut self,
        nanos: Nanoseconds,
        modifier::Subsecond { digits }: modifier::Subsecond,
    ) -> Result<(), io::Error> {
        use modifier::SubsecondDigits::*;

        #[repr(C, align(8))]
        #[derive(Clone, Copy)]
        struct Digits {
            _padding: MaybeUninit<[u8; 7]>,
            digit_1: u8,
            digits_2_thru_9: [u8; 8],
        }

        let [
            digit_1,
            digits_2_and_3,
            digits_4_and_5,
            digits_6_and_7,
            digits_8_and_9,
        ] = num_fmt::subsecond_from_nanos(nanos);

        // Ensure that digits 2 thru 9 are stored as a single array that is 8-aligned. This allows
        // the conversion to a `u64` to be zero cost, resulting in a nontrivial performance
        // improvement.
        let buf = Digits {
            _padding: MaybeUninit::uninit(),
            digit_1: digit_1.as_bytes()[0],
            digits_2_thru_9: [
                digits_2_and_3.as_bytes()[0],
                digits_2_and_3.as_bytes()[1],
                digits_4_and_5.as_bytes()[0],
                digits_4_and_5.as_bytes()[1],
                digits_6_and_7.as_bytes()[0],
                digits_6_and_7.as_bytes()[1],
                digits_8_and_9.as_bytes()[0],
                digits_8_and_9.as_bytes()[1],
            ],
        };

        let len = match digits {
            One => 1,
            Two => 2,
            Three => 3,
            Four => 4,
            Five => 5,
            Six => 6,
            Seven => 7,
            Eight => 8,
            Nine => 9,
            OneOrMore => {
                // By converting the bytes into a single integer, we can effectively perform an
                // equality check against b'0' for all bytes at once. This is
                // actually faster than using portable SIMD (even with
                // `-Ctarget-cpu=native`).
                let bitmask =
                    u64::from_le_bytes(buf.digits_2_thru_9) ^ u64::from_le_bytes([b'0'; 8]);
                let digits_to_truncate = bitmask.leading_zeros() / 8;
                9 - digits_to_truncate as usize
            }
        };

        // Safety: All bytes are initialized and valid UTF-8, and `len` represents the number of
        // bytes we wish to display (that is between 1 and 9 inclusive). `Digits` is
        // `#[repr(C)]`, so the layout is guaranteed.
        let s = unsafe {
            num_fmt::StackStr::new(
                *(&raw const buf)
                    .byte_add(core::mem::offset_of!(Digits, digit_1))
                    .cast::<[MaybeUninit<u8>; 9]>(),
                len,
            )
        };
        self.write(&s)
    }

    #[inline]
    fn fmt_sign(&mut self, is_negative: bool, sign_is_mandatory: bool) -> io::Result<()> {
        if is_negative {
            self.write("-")
        } else if sign_is_mandatory {
            self.write("+")
        } else {
            Ok(())
        }
    }

    /// Format the offset hour into the designated output.
    #[inline]
    fn fmt_offset_hour(
        &mut self,
        is_negative: bool,
        hour: OffsetHours,
        modifier::OffsetHour {
            padding,
            sign_is_mandatory,
        }: modifier::OffsetHour,
    ) -> Result<(), io::Error> {
        try_likely_ok!(self.fmt_sign(is_negative, sign_is_mandatory));
        try_likely_ok!(self.format_two_digits(
            // Safety: The value is guaranteed to be under 100 because of `OffsetHours`.
            unsafe { ru8::new_unchecked(hour.get().unsigned_abs()) },
            padding,
        ));
        Ok(())
    }

    /// Format the offset minute into the designated output.
    #[inline]
    fn fmt_offset_minute(
        &mut self,
        offset_minute: OffsetMinutes,
        modifier::OffsetMinute { padding }: modifier::OffsetMinute,
    ) -> Result<(), io::Error> {
        self.format_two_digits(
            // Safety: `OffsetMinutes` is guaranteed to be in the range `-59..=59`, so the absolute
            // value is guaranteed to be in the range `0..=59`.
            unsafe { ru8::new_unchecked(offset_minute.get().unsigned_abs()) },
            padding,
        )
    }

    /// Format the offset second into the designated output.
    #[inline]
    fn fmt_offset_second(
        &mut self,
        offset_second: OffsetSeconds,
        modifier::OffsetSecond { padding }: modifier::OffsetSecond,
    ) -> Result<(), io::Error> {
        self.format_two_digits(
            // Safety: `OffsetSeconds` is guaranteed to be in the range `-59..=59`, so the absolute
            // value is guaranteed to be in the range `0..=59`.
            unsafe { ru8::new_unchecked(offset_second.get().unsigned_abs()) },
            padding,
        )
    }

    /// Format the Unix timestamp (in seconds) into the designated output.
    #[inline]
    fn fmt_unix_timestamp_second(
        &mut self,
        timestamp: i64,
        modifier::UnixTimestampSecond { sign_is_mandatory }: modifier::UnixTimestampSecond,
    ) -> Result<(), io::Error> {
        try_likely_ok!(self.fmt_sign(timestamp < 0, sign_is_mandatory));
        try_likely_ok!(self.format_u64_pad_none(timestamp.unsigned_abs()));
        Ok(())
    }

    /// Format the Unix timestamp (in milliseconds) into the designated output.
    #[inline]
    fn fmt_unix_timestamp_millisecond(
        &mut self,
        timestamp_millis: i64,
        modifier::UnixTimestampMillisecond { sign_is_mandatory }:
            modifier::UnixTimestampMillisecond,
    ) -> Result<(), io::Error> {
        try_likely_ok!(self.fmt_sign(timestamp_millis < 0, sign_is_mandatory));
        try_likely_ok!(self.format_u64_pad_none(timestamp_millis.unsigned_abs()));
        Ok(())
    }

    /// Format the Unix timestamp (in microseconds) into the designated output.
    #[inline]
    fn fmt_unix_timestamp_microsecond(
        &mut self,
        timestamp_micros: i128,
        modifier::UnixTimestampMicrosecond { sign_is_mandatory }:
            modifier::UnixTimestampMicrosecond,
    ) -> Result<(), io::Error> {
        try_likely_ok!(self.fmt_sign(timestamp_micros < 0, sign_is_mandatory));
        try_likely_ok!(self.format_u128_pad_none(timestamp_micros.unsigned_abs()));
        Ok(())
    }

    /// Format the Unix timestamp (in nanoseconds) into the designated output.
    #[inline]
    fn fmt_unix_timestamp_nanosecond(
        &mut self,
        timestamp_nanos: i128,
        modifier::UnixTimestampNanosecond { sign_is_mandatory }: modifier::UnixTimestampNanosecond,
    ) -> Result<(), io::Error> {
        try_likely_ok!(self.fmt_sign(timestamp_nanos < 0, sign_is_mandatory));
        try_likely_ok!(self.format_u128_pad_none(timestamp_nanos.unsigned_abs()));
        Ok(())
    }
}
