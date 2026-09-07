//! A trait that can be used to format an item from its components.

use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Deref;
use std::io;

use deranged::{ru8, ru16};
use num_conv::prelude::*;

use crate::format_description::format_description_v3::FormatDescriptionV3Inner;
use crate::format_description::modifier::Padding;
use crate::format_description::well_known::iso8601::EncodedConfig;
use crate::format_description::well_known::{Iso8601, Rfc2822, Rfc3339};
use crate::format_description::{
    BorrowedFormatItem, Component, FormatDescriptionV3, OwnedFormatItem,
};
use crate::formatting::{ComponentProvider, MONTH_NAMES, Output, WEEKDAY_NAMES};
use crate::internal_macros::try_likely_ok;
use crate::{PrivateMethod, error, num_fmt};

macro_rules! fmt_component_match {
    (
        $self:expr,
        $output:ident,
        $value:ident,
        $state:ident,
        $($extra:tt)*
    ) => {
        match $self {
            Self::Day(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_day($value.day($state), *modifier).map_err(Into::into)
            }
            Self::MonthShort(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_month_short($value.month($state), *modifier).map_err(Into::into)
            }
            Self::MonthLong(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_month_long($value.month($state), *modifier).map_err(Into::into)
            }
            Self::MonthNumerical(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_month_numerical($value.month($state), *modifier).map_err(Into::into)
            }
            Self::Ordinal(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_ordinal($value.ordinal($state), *modifier).map_err(Into::into)
            }
            Self::WeekdayShort(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_weekday_short($value.weekday($state), *modifier).map_err(Into::into)
            }
            Self::WeekdayLong(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_weekday_long($value.weekday($state), *modifier).map_err(Into::into)
            }
            Self::WeekdaySunday(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_weekday_sunday($value.weekday($state), *modifier).map_err(Into::into)
            }
            Self::WeekdayMonday(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_weekday_monday($value.weekday($state), *modifier).map_err(Into::into)
            }
            Self::WeekNumberIso(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_week_number_iso(
                    $value.iso_week_number($state),
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::WeekNumberSunday(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_week_number_sunday(
                    $value.sunday_based_week($state),
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::WeekNumberMonday(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_week_number_monday(
                    $value.monday_based_week($state),
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::CalendarYearFullExtendedRange(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_calendar_year_full_extended_range(
                    $value.calendar_year($state),
                    *modifier
                ).map_err(Into::into)
            }
            Self::CalendarYearFullStandardRange(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_calendar_year_full_standard_range(
                    try_likely_ok!(
                        $value
                            .calendar_year($state)
                            .narrow::<-9_999, 9_999>()
                            .ok_or_else(|| error::ComponentRange::conditional("year"))
                    )
                    .into(),
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::IsoYearFullExtendedRange(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_iso_year_full_extended_range(
                    $value.iso_year($state),
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::IsoYearFullStandardRange(modifier) if V::SUPPLIES_DATE => {
                $output.fmt_iso_year_full_standard_range(
                    try_likely_ok!(
                        $value
                            .iso_year($state)
                            .narrow::<-9_999, 9_999>()
                            .ok_or_else(|| error::ComponentRange::conditional("year"))
                    )
                    .into(),
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::CalendarYearCenturyExtendedRange(modifier) if V::SUPPLIES_DATE => {
                let year = $value.calendar_year($state);
                // Safety: Given the range of `year`, the range of the century is `-9_999..=9_999`.
                let century = unsafe { ri16::new_unchecked((year.get() / 100).truncate()) };
                $output.fmt_calendar_year_century_extended_range(
                    century,
                    year.is_negative(),
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::CalendarYearCenturyStandardRange(modifier) if V::SUPPLIES_DATE => {
                let year = $value.calendar_year($state);
                let is_negative = year.is_negative();
                // Safety: Given the range of `year`, the range of the century is `-9_999..=9_999`.
                let year = unsafe {
                    ri16::<-9_999, 9_999>::new_unchecked((year.get() / 100).truncate())
                };
                $output.fmt_calendar_year_century_standard_range(
                    year.narrow::<-99, 99>()
                        .ok_or_else(|| error::ComponentRange::conditional("year"))?
                        .into(),
                    is_negative,
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::IsoYearCenturyExtendedRange(modifier) if V::SUPPLIES_DATE => {
                let year = $value.iso_year($state);
                let is_negative = year.is_negative();
                // Safety: Given the range of `year`, the range of the century is `-9_999..=9_999`.
                let century = unsafe { ri16::new_unchecked((year.get() / 100).truncate()) };
                $output.fmt_iso_year_century_extended_range(
                    century,
                    is_negative,
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::IsoYearCenturyStandardRange(modifier) if V::SUPPLIES_DATE => {
                let year = $value.iso_year($state);
                let is_negative = year.is_negative();
                // Safety: Given the range of `year`, the range of the century is `-9_999..=9_999`.
                let year = unsafe {
                    ri16::<-9_999, 9_999>::new_unchecked((year.get() / 100).truncate())
                };
                $output.fmt_iso_year_century_standard_range(
                    year.narrow::<-99, 99>()
                        .ok_or_else(|| error::ComponentRange::conditional("year"))?
                        .into(),
                    is_negative,
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::CalendarYearLastTwo(modifier) if V::SUPPLIES_DATE => {
                // Safety: Modulus of 100 followed by `.unsigned_abs()` guarantees that the $value
                // is in the range `0..=99`.
                let last_two = unsafe {
                    ru8::new_unchecked(
                        ($value.calendar_year($state).get().unsigned_abs() % 100).truncate(),
                    )
                };
                $output.fmt_calendar_year_last_two(last_two, *modifier)
                    .map_err(Into::into)
            }
            Self::IsoYearLastTwo(modifier) if V::SUPPLIES_DATE => {
                // Safety: Modulus of 100 followed by `.unsigned_abs()` guarantees that the $value
                // is in the range `0..=99`.
                let last_two = unsafe {
                    ru8::new_unchecked(
                        ($value.iso_year($state).get().unsigned_abs() % 100).truncate(),
                    )
                };
                $output.fmt_iso_year_last_two(last_two, *modifier).map_err(Into::into)
            }
            Self::Hour12(modifier) if V::SUPPLIES_TIME => {
                $output.fmt_hour_12($value.hour($state), *modifier).map_err(Into::into)
            }
            Self::Hour24(modifier) if V::SUPPLIES_TIME => {
                $output.fmt_hour_24($value.hour($state), *modifier).map_err(Into::into)
            }
            Self::Minute(modifier) if V::SUPPLIES_TIME => {
                $output.fmt_minute($value.minute($state), *modifier).map_err(Into::into)
            }
            Self::Period(modifier) if V::SUPPLIES_TIME => {
                $output.fmt_period($value.period($state), *modifier).map_err(Into::into)
            }
            Self::Second(modifier) if V::SUPPLIES_TIME => {
                $output.fmt_second($value.second($state), *modifier).map_err(Into::into)
            }
            Self::Subsecond(modifier) if V::SUPPLIES_TIME => {
                $output.fmt_subsecond($value.nanosecond($state), *modifier).map_err(Into::into)
            }
            Self::OffsetHour(modifier) if V::SUPPLIES_OFFSET => {
                $output.fmt_offset_hour(
                    $value.offset_is_negative($state),
                    $value.offset_hour($state),
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::OffsetMinute(modifier) if V::SUPPLIES_OFFSET => {
                $output.fmt_offset_minute($value.offset_minute($state), *modifier)
                    .map_err(Into::into)
            }
            Self::OffsetSecond(modifier) if V::SUPPLIES_OFFSET => {
                $output.fmt_offset_second($value.offset_second($state), *modifier)
                    .map_err(Into::into)
            }
            Self::Ignore(_) => Ok(()),
            Self::UnixTimestampSecond(modifier) if V::SUPPLIES_TIMESTAMP => {
                $output.fmt_unix_timestamp_second(
                    $value.unix_timestamp_seconds($state),
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::UnixTimestampMillisecond(modifier) if V::SUPPLIES_TIMESTAMP => {
                $output.fmt_unix_timestamp_millisecond(
                    $value.unix_timestamp_milliseconds($state),
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::UnixTimestampMicrosecond(modifier) if V::SUPPLIES_TIMESTAMP => {
                $output.fmt_unix_timestamp_microsecond(
                    $value.unix_timestamp_microseconds($state),
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::UnixTimestampNanosecond(modifier) if V::SUPPLIES_TIMESTAMP => {
                $output.fmt_unix_timestamp_nanosecond(
                    $value.unix_timestamp_nanoseconds($state),
                    *modifier,
                )
                .map_err(Into::into)
            }
            Self::End(modifier::End { trailing_input: _ }) => Ok(()),
            $($extra)*
        }
    };
}

/// A type that describes a format.
///
/// Implementors of [`Formattable`] are [format descriptions](crate::format_description).
///
/// To format a value into a String, use the `format` method on the respective type.
#[cfg_attr(docsrs, doc(notable_trait))]
pub trait Formattable: sealed::Sealed {}
impl Formattable for FormatDescriptionV3<'_> {}
impl Formattable for BorrowedFormatItem<'_> {}
impl Formattable for [BorrowedFormatItem<'_>] {}
impl Formattable for OwnedFormatItem {}
impl Formattable for [OwnedFormatItem] {}
impl Formattable for Rfc3339 {}
impl Formattable for Rfc2822 {}
impl<const CONFIG: EncodedConfig> Formattable for Iso8601<CONFIG> {}
impl<T> Formattable for T where T: Deref<Target: Formattable> {}

/// Seal the trait to prevent downstream users from implementing it.
mod sealed {
    use super::*;
    use crate::formatting::ComponentProvider;
    use crate::formatting::metadata::ComputeMetadata;

    /// Format the item using a format description, the intended output, and the various components.
    #[expect(
        private_bounds,
        private_interfaces,
        reason = "irrelevant due to being a sealed trait"
    )]
    pub trait Sealed: ComputeMetadata {
        /// Format the item into the provided output.
        fn format_into<V>(
            &self,
            output: &mut Output<impl io::Write + ?Sized>,
            value: &V,
            state: &mut V::State,
            _: PrivateMethod,
        ) -> Result<(), error::Format>
        where
            V: ComponentProvider;

        /// Format the item directly to a `String`.
        #[inline]
        fn format<V>(
            &self,
            value: &V,
            state: &mut V::State,
            _: PrivateMethod,
        ) -> Result<String, error::Format>
        where
            V: ComponentProvider,
        {
            let crate::formatting::metadata::Metadata {
                max_bytes_needed,
                guaranteed_utf8,
            } = self.compute_metadata(PrivateMethod);

            let mut output = Output {
                bytes_written: 0,
                output: Vec::with_capacity(max_bytes_needed),
            };
            try_likely_ok!(self.format_into(&mut output, value, state, PrivateMethod));
            Ok(if guaranteed_utf8 {
                // Safety: The output is guaranteed to be UTF-8.
                unsafe { String::from_utf8_unchecked(output.output) }
            } else {
                String::from_utf8_lossy(&output.output).into_owned()
            })
        }
    }
}

impl sealed::Sealed for FormatDescriptionV3<'_> {
    #[expect(
        private_bounds,
        private_interfaces,
        reason = "irrelevant due to being a sealed trait"
    )]
    #[inline]
    fn format_into<V>(
        &self,
        output: &mut Output<impl io::Write + ?Sized>,
        value: &V,
        state: &mut V::State,
        _: PrivateMethod,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        self.inner.format_into(output, value, state, PrivateMethod)
    }
}

impl sealed::Sealed for FormatDescriptionV3Inner<'_> {
    #[expect(
        private_bounds,
        private_interfaces,
        reason = "irrelevant due to being a sealed trait"
    )]
    #[inline]
    fn format_into<V>(
        &self,
        output: &mut Output<impl io::Write + ?Sized>,
        value: &V,
        state: &mut V::State,
        _: PrivateMethod,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        use crate::formatting::*;

        fmt_component_match! { &self, output, value, state,
            Self::BorrowedLiteral(literal) => output.write(literal).map_err(Into::into),
            Self::BorrowedCompound(items) => {
                for item in *items {
                    try_likely_ok!(item.format_into(
                        output,
                        value,
                        state,
                        PrivateMethod,
                    ));
                }
                Ok(())
            }
            Self::BorrowedOptional {
                format: should_format,
                item,
            } => {
                if *should_format {
                    item.format_into(output, value, state, PrivateMethod)
                } else {
                    Ok(())
                }
            }
            Self::BorrowedFirst(items) => match items {
                [] => Ok(()),
                [item, ..] => {
                    item.format_into(output, value, state, PrivateMethod)
                }
            },
            Self::OwnedLiteral(literal) => output.write(literal).map_err(Into::into),
            Self::OwnedCompound(items) => {
                for item in &**items {
                    try_likely_ok!(item.format_into(
                        output,
                        value,
                        state,
                        PrivateMethod,
                    ));
                }
                Ok(())
            }
            Self::OwnedOptional {
                format: should_format,
                item,
            } => {
                if *should_format {
                    item.format_into(output, value, state, PrivateMethod)
                } else {
                    Ok(())
                }
            }
            Self::OwnedFirst(items) => match &items[..] {
                [] => Ok(()),
                [item, ..] => {
                    item.format_into(output, value, state, PrivateMethod)
                }
            },

            // This is functionally the same as a wildcard arm, but it will cause an error
            // if a new component is added. This is to avoid a bug where
            // a new component, the code compiles, and formatting fails.
            // Allow unreachable patterns because some branches may be fully matched above.
            #[allow(unreachable_patterns)]
            Self::Day(_)
            | Self::MonthShort(_)
            | Self::MonthLong(_)
            | Self::MonthNumerical(_)
            | Self::Ordinal(_)
            | Self::WeekdayShort(_)
            | Self::WeekdayLong(_)
            | Self::WeekdaySunday(_)
            | Self::WeekdayMonday(_)
            | Self::WeekNumberIso(_)
            | Self::WeekNumberSunday(_)
            | Self::WeekNumberMonday(_)
            | Self::CalendarYearFullExtendedRange(_)
            | Self::CalendarYearFullStandardRange(_)
            | Self::IsoYearFullExtendedRange(_)
            | Self::IsoYearFullStandardRange(_)
            | Self::CalendarYearCenturyExtendedRange(_)
            | Self::CalendarYearCenturyStandardRange(_)
            | Self::IsoYearCenturyExtendedRange(_)
            | Self::IsoYearCenturyStandardRange(_)
            | Self::CalendarYearLastTwo(_)
            | Self::IsoYearLastTwo(_)
            | Self::Hour12(_)
            | Self::Hour24(_)
            | Self::Minute(_)
            | Self::Period(_)
            | Self::Second(_)
            | Self::Subsecond(_)
            | Self::OffsetHour(_)
            | Self::OffsetMinute(_)
            | Self::OffsetSecond(_)
            | Self::Ignore(_)
            | Self::UnixTimestampSecond(_)
            | Self::UnixTimestampMillisecond(_)
            | Self::UnixTimestampMicrosecond(_)
            | Self::UnixTimestampNanosecond(_)
            | Self::End(_) => Err(error::Format::InsufficientTypeInformation),
        }
    }
}

impl Component {
    /// Format the component directly into the provided output.
    #[inline]
    #[allow(deprecated)]
    pub(crate) fn format_into<V>(
        &self,
        output: &mut Output<impl io::Write + ?Sized>,
        value: &V,
        state: &mut V::State,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        use sealed::Sealed;

        use crate::formatting::*;

        fmt_component_match! { self, output, value, state,
            // Deprecated component variants: delegate through the existing conversion.
            Self::Month(_) | Self::Weekday(_) | Self::WeekNumber(_)
                if V::SUPPLIES_DATE =>
            {
                FormatDescriptionV3Inner::from(*self)
                    .format_into(output, value, state, PrivateMethod)
            }
            Self::Hour(_) if V::SUPPLIES_TIME => {
                FormatDescriptionV3Inner::from(*self)
                    .format_into(output, value, state, PrivateMethod)
            }
            Self::UnixTimestamp(_) if V::SUPPLIES_TIMESTAMP => {
                FormatDescriptionV3Inner::from(*self)
                    .format_into(output, value, state, PrivateMethod)
            }
            Self::Year(_) if V::SUPPLIES_DATE => {
                FormatDescriptionV3Inner::from(*self)
                    .format_into(output, value, state, PrivateMethod)
            }

            // Unmatched component (e.g. time component on a date-only type).
            #[allow(unreachable_patterns)]
            Self::Day(_)
            | Self::MonthShort(_)
            | Self::MonthLong(_)
            | Self::MonthNumerical(_)
            | Self::Ordinal(_)
            | Self::WeekdayShort(_)
            | Self::WeekdayLong(_)
            | Self::WeekdaySunday(_)
            | Self::WeekdayMonday(_)
            | Self::WeekNumberIso(_)
            | Self::WeekNumberSunday(_)
            | Self::WeekNumberMonday(_)
            | Self::CalendarYearFullExtendedRange(_)
            | Self::CalendarYearFullStandardRange(_)
            | Self::IsoYearFullExtendedRange(_)
            | Self::IsoYearFullStandardRange(_)
            | Self::CalendarYearCenturyExtendedRange(_)
            | Self::CalendarYearCenturyStandardRange(_)
            | Self::IsoYearCenturyExtendedRange(_)
            | Self::IsoYearCenturyStandardRange(_)
            | Self::CalendarYearLastTwo(_)
            | Self::IsoYearLastTwo(_)
            | Self::Hour12(_)
            | Self::Hour24(_)
            | Self::Minute(_)
            | Self::Period(_)
            | Self::Second(_)
            | Self::Subsecond(_)
            | Self::OffsetHour(_)
            | Self::OffsetMinute(_)
            | Self::OffsetSecond(_)
            | Self::Ignore(_)
            | Self::UnixTimestampSecond(_)
            | Self::UnixTimestampMillisecond(_)
            | Self::UnixTimestampMicrosecond(_)
            | Self::UnixTimestampNanosecond(_)
            // Deprecated variants not matched by guarded arms above.
            | Self::Month(_)
            | Self::Weekday(_)
            | Self::WeekNumber(_)
            | Self::Hour(_)
            | Self::UnixTimestamp(_)
            | Self::Year(_) => Err(error::Format::InsufficientTypeInformation),
        }
    }
}

impl sealed::Sealed for BorrowedFormatItem<'_> {
    #[expect(
        private_bounds,
        private_interfaces,
        reason = "irrelevant due to being a sealed trait"
    )]
    #[inline]
    fn format_into<V>(
        &self,
        output: &mut Output<impl io::Write + ?Sized>,
        value: &V,
        state: &mut V::State,
        _: PrivateMethod,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        match *self {
            #[expect(deprecated)]
            Self::Literal(literal) => try_likely_ok!(output.write_bytes(literal)),
            Self::StringLiteral(literal) => try_likely_ok!(output.write(literal)),
            Self::Component(component) => component.format_into(output, value, state)?,
            Self::Compound(items) => {
                try_likely_ok!((*items).format_into(output, value, state, PrivateMethod));
            }
            Self::Optional(item) => {
                try_likely_ok!((*item).format_into(output, value, state, PrivateMethod));
            }
            Self::First(items) => match items {
                [] => {}
                [item, ..] => {
                    try_likely_ok!((*item).format_into(output, value, state, PrivateMethod));
                }
            },
        }
        Ok(())
    }
}

impl sealed::Sealed for [BorrowedFormatItem<'_>] {
    #[expect(
        private_bounds,
        private_interfaces,
        reason = "irrelevant due to being a sealed trait"
    )]
    #[inline]
    fn format_into<V>(
        &self,
        output: &mut Output<impl io::Write + ?Sized>,
        value: &V,
        state: &mut V::State,
        _: PrivateMethod,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        for item in self.iter() {
            try_likely_ok!(item.format_into(output, value, state, PrivateMethod));
        }
        Ok(())
    }
}

impl sealed::Sealed for OwnedFormatItem {
    #[expect(
        private_bounds,
        private_interfaces,
        reason = "irrelevant due to being a sealed trait"
    )]
    #[inline]
    fn format_into<V>(
        &self,
        output: &mut Output<impl io::Write + ?Sized>,
        value: &V,
        state: &mut V::State,
        _: PrivateMethod,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        match self {
            #[expect(deprecated)]
            Self::Literal(literal) => output.write_bytes(literal).map_err(Into::into),
            Self::StringLiteral(literal) => output.write(literal).map_err(Into::into),
            Self::Component(component) => FormatDescriptionV3Inner::<'_>::from(*component)
                .format_into(output, value, state, PrivateMethod),
            Self::Compound(items) => (**items).format_into(output, value, state, PrivateMethod),
            Self::Optional(item) => (**item).format_into(output, value, state, PrivateMethod),
            Self::First(items) => match &**items {
                [] => Ok(()),
                [item, ..] => (*item).format_into(output, value, state, PrivateMethod),
            },
        }
    }
}

impl sealed::Sealed for [OwnedFormatItem] {
    #[expect(
        private_bounds,
        private_interfaces,
        reason = "irrelevant due to being a sealed trait"
    )]
    #[inline]
    fn format_into<V>(
        &self,
        output: &mut Output<impl io::Write + ?Sized>,
        value: &V,
        state: &mut V::State,
        _: PrivateMethod,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        for item in self.iter() {
            try_likely_ok!(item.format_into(output, value, state, PrivateMethod));
        }
        Ok(())
    }
}

impl<T> sealed::Sealed for T
where
    T: Deref<Target: sealed::Sealed>,
{
    #[expect(
        private_bounds,
        private_interfaces,
        reason = "irrelevant due to being a sealed trait"
    )]
    #[inline]
    fn format_into<V>(
        &self,
        output: &mut Output<impl io::Write + ?Sized>,
        value: &V,
        state: &mut V::State,
        _: PrivateMethod,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        self.deref()
            .format_into(output, value, state, PrivateMethod)
    }
}

#[expect(
    private_bounds,
    private_interfaces,
    reason = "irrelevant due to being a sealed trait"
)]
impl sealed::Sealed for Rfc2822 {
    fn format_into<V>(
        &self,
        output: &mut Output<impl io::Write + ?Sized>,
        value: &V,
        state: &mut V::State,
        _: PrivateMethod,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        const {
            assert!(
                V::SUPPLIES_DATE && V::SUPPLIES_TIME && V::SUPPLIES_OFFSET,
                "Rfc2822 requires date, time, and offset components, but not all can be provided \
                 by this type"
            );
        }

        if value.calendar_year(state).get() < 1900
            // The RFC requires years be exactly four digits.
            || (cfg!(feature = "large-dates") && value.calendar_year(state).get() >= 10_000)
        {
            crate::hint::cold_path();
            return Err(error::Format::InvalidComponent("year"));
        }
        if value.offset_second(state).get() != 0 {
            crate::hint::cold_path();
            return Err(error::Format::InvalidComponent("offset_second"));
        }

        // Safety: All weekday names are at least 3 bytes long.
        try_likely_ok!(output.write(unsafe {
            WEEKDAY_NAMES[value
                .weekday(state)
                .number_days_from_monday()
                .widen::<usize>()]
            .get_unchecked(..3)
        }));
        try_likely_ok!(output.write(", "));
        try_likely_ok!(output.format_two_digits(value.day(state).expand(), Padding::Zero));
        try_likely_ok!(output.write(" "));
        // Safety: All month names are at least 3 bytes long.
        try_likely_ok!(output.write(unsafe {
            MONTH_NAMES[u8::from(value.month(state)).widen::<usize>() - 1].get_unchecked(..3)
        }));
        try_likely_ok!(output.write(" "));
        // Safety: Years with five or more digits were rejected above. Likewise with negative years.
        try_likely_ok!(output.format_four_digits_pad_zero(unsafe {
            ru16::new_unchecked(value.calendar_year(state).get().cast_unsigned().truncate())
        }));
        try_likely_ok!(output.write(" "));
        try_likely_ok!(output.format_two_digits(value.hour(state).expand(), Padding::Zero));
        try_likely_ok!(output.write(":"));
        try_likely_ok!(output.format_two_digits(value.minute(state).expand(), Padding::Zero));
        try_likely_ok!(output.write(":"));
        try_likely_ok!(output.format_two_digits(value.second(state).expand(), Padding::Zero));
        try_likely_ok!(output.write(" "));
        try_likely_ok!(output.write_if_else(value.offset_is_negative(state), "-", "+"));
        try_likely_ok!(output.format_two_digits(
            // Safety: `OffsetHours` is guaranteed to be in the range `-25..=25`, so the absolute
            // value is guaranteed to be in the range `0..=25`.
            unsafe { ru8::new_unchecked(value.offset_hour(state).get().unsigned_abs()) },
            Padding::Zero,
        ));
        try_likely_ok!(output.format_two_digits(
            // Safety: `OffsetMinutes` is guaranteed to be in the range `-59..=59`, so the absolute
            // value is guaranteed to be in the range `0..=59`.
            unsafe { ru8::new_unchecked(value.offset_minute(state).get().unsigned_abs()) },
            Padding::Zero,
        ));

        Ok(())
    }
}

#[expect(
    private_bounds,
    private_interfaces,
    reason = "irrelevant due to being a sealed trait"
)]
impl sealed::Sealed for Rfc3339 {
    fn format_into<V>(
        &self,
        output: &mut Output<impl io::Write + ?Sized>,
        value: &V,
        state: &mut V::State,
        _: PrivateMethod,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        const {
            assert!(
                V::SUPPLIES_DATE && V::SUPPLIES_TIME && V::SUPPLIES_OFFSET,
                "Rfc3339 requires date, time, and offset components, but not all can be provided \
                 by this type"
            );
        }

        let offset_hour = value.offset_hour(state);

        if !(0..10_000).contains(&value.calendar_year(state).get()) {
            crate::hint::cold_path();
            return Err(error::Format::InvalidComponent("year"));
        }
        if offset_hour.get().unsigned_abs() > 23 {
            crate::hint::cold_path();
            return Err(error::Format::InvalidComponent("offset_hour"));
        }
        if value.offset_second(state).get() != 0 {
            crate::hint::cold_path();
            return Err(error::Format::InvalidComponent("offset_second"));
        }

        // Safety: Years outside this range were rejected above.
        try_likely_ok!(output.format_four_digits_pad_zero(unsafe {
            ru16::new_unchecked(value.calendar_year(state).get().cast_unsigned().truncate())
        }));
        try_likely_ok!(output.write("-"));
        try_likely_ok!(output.format_two_digits(
            // Safety: `month` is guaranteed to be in the range `1..=12`.
            unsafe { ru8::new_unchecked(u8::from(value.month(state))) },
            Padding::Zero,
        ));
        try_likely_ok!(output.write("-"));
        try_likely_ok!(output.format_two_digits(value.day(state).expand(), Padding::Zero));
        try_likely_ok!(output.write("T"));
        try_likely_ok!(output.format_two_digits(value.hour(state).expand(), Padding::Zero));
        try_likely_ok!(output.write(":"));
        try_likely_ok!(output.format_two_digits(value.minute(state).expand(), Padding::Zero));
        try_likely_ok!(output.write(":"));
        try_likely_ok!(output.format_two_digits(value.second(state).expand(), Padding::Zero));

        let nanos = value.nanosecond(state);
        if nanos.get() != 0 {
            try_likely_ok!(output.write("."));
            try_likely_ok!(output.write(&num_fmt::truncated_subsecond_from_nanos(nanos)));
        }

        if value.offset_is_utc(state) {
            try_likely_ok!(output.write("Z"));
            return Ok(());
        }

        try_likely_ok!(output.write_if_else(value.offset_is_negative(state), "-", "+"));
        try_likely_ok!(output.format_two_digits(
            // Safety: `OffsetHours` is guaranteed to be in the range `-23..=23`, so the absolute
            // value is guaranteed to be in the range `0..=23`.
            unsafe { ru8::new_unchecked(offset_hour.get().unsigned_abs()) },
            Padding::Zero,
        ));
        try_likely_ok!(output.write(":"));
        try_likely_ok!(output.format_two_digits(
            // Safety: `OffsetMinutes` is guaranteed to be in the range `-59..=59`, so the absolute
            // value is guaranteed to be in the range `0..=59`.
            unsafe { ru8::new_unchecked(value.offset_minute(state).get().unsigned_abs()) },
            Padding::Zero,
        ));

        Ok(())
    }
}

impl<const CONFIG: EncodedConfig> sealed::Sealed for Iso8601<CONFIG> {
    #[expect(
        private_bounds,
        private_interfaces,
        reason = "irrelevant due to being a sealed trait"
    )]
    #[inline]
    fn format_into<V>(
        &self,
        output: &mut Output<impl io::Write + ?Sized>,
        value: &V,
        state: &mut V::State,
        _: PrivateMethod,
    ) -> Result<(), error::Format>
    where
        V: ComponentProvider,
    {
        const {
            assert!(
                !Self::FORMAT_DATE || V::SUPPLIES_DATE,
                "this Iso8601 configuration formats date components, but this type cannot provide \
                 them"
            );
            assert!(
                !Self::FORMAT_TIME || V::SUPPLIES_TIME,
                "this Iso8601 configuration formats time components, but this type cannot provide \
                 them"
            );
            assert!(
                !Self::FORMAT_OFFSET || V::SUPPLIES_OFFSET,
                "this Iso8601 configuration formats offset components, but this type cannot \
                 provide them"
            );
            assert!(
                Self::FORMAT_DATE || Self::FORMAT_TIME || Self::FORMAT_OFFSET,
                "this Iso8601 configuration does not format any components"
            );
        }

        if Self::FORMAT_DATE {
            output.format_iso8601_date::<_, CONFIG>(value, state)?;
        }
        if Self::FORMAT_TIME {
            output.format_iso8601_time::<_, CONFIG>(value, state)?;
        }
        if Self::FORMAT_OFFSET {
            output.format_iso8601_offset::<_, CONFIG>(value, state)?;
        }

        Ok(())
    }
}
