//! Whole-second calendar representation using Embassy names and accessors.

/// Calendar validation failures.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// CW32 has a two-digit year. This API represents only 2000..=2099.
    InvalidYear,
    /// Month must be 1..=12.
    InvalidMonth,
    /// The day does not exist in this month/year.
    InvalidDay,
    /// Weekday encoding is invalid or disagrees with the Gregorian date.
    InvalidDayOfWeek(u8),
    /// Hour must be 0..=23.
    InvalidHour,
    /// Minute must be 0..=59.
    InvalidMinute,
    /// Second must be 0..=59. Leap seconds are unsupported.
    InvalidSecond,
    /// This whole-second API accepts microsecond=0 only.
    InvalidMicrosecond,
    /// A stored calendar field has a non-decimal BCD digit.
    InvalidBcd,
}

/// A weekday, with the Embassy public discriminants (Monday=1, Sunday=7).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DayOfWeek {
    /// Monday.
    Monday = 1,
    /// Tuesday.
    Tuesday,
    /// Wednesday.
    Wednesday,
    /// Thursday.
    Thursday,
    /// Friday.
    Friday,
    /// Saturday.
    Saturday,
    /// Sunday. CW32's hardware encoding is 0; conversion is explicit.
    Sunday,
}
impl DayOfWeek {
    pub(super) fn from_hardware(value: u8) -> Result<Self, Error> {
        Ok(match value {
            0 => Self::Sunday,
            1 => Self::Monday,
            2 => Self::Tuesday,
            3 => Self::Wednesday,
            4 => Self::Thursday,
            5 => Self::Friday,
            6 => Self::Saturday,
            n => return Err(Error::InvalidDayOfWeek(n)),
        })
    }
    pub(super) const fn hardware(self) -> u8 {
        self as u8 % 7
    }
}

/// Valid Gregorian whole-second date/time in the software epoch 2000..=2099.
///
/// No century is stored on CW32. After 2099-12-31 hardware rolls its year back
/// to 00; the application must deal with that boundary rather than infer 2100.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DateTime {
    year: u16,
    month: u8,
    day: u8,
    day_of_week: DayOfWeek,
    hour: u8,
    minute: u8,
    second: u8,
}
impl DateTime {
    /// Match Embassy's constructor shape; only zero microseconds are supported.
    /// Weekday must agree with the Gregorian date, including leap-day rules.
    #[allow(clippy::too_many_arguments)]
    pub fn from(
        year: u16,
        month: u8,
        day: u8,
        day_of_week: DayOfWeek,
        hour: u8,
        minute: u8,
        second: u8,
        usecond: u32,
    ) -> Result<Self, Error> {
        if !(2000..=2099).contains(&year) {
            return Err(Error::InvalidYear);
        }
        if !(1..=12).contains(&month) {
            return Err(Error::InvalidMonth);
        }
        let days = match month {
            2 if year % 4 == 0 => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        if day == 0 || day > days {
            return Err(Error::InvalidDay);
        }
        let before_month =
            [0u32, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334][month as usize - 1];
        let years = u32::from(year - 2000);
        let elapsed = 365 * years
            + (years + 3) / 4
            + before_month
            + u32::from(month > 2 && year % 4 == 0)
            + u32::from(day - 1);
        // 2000-01-01 was Saturday (hardware 6).
        if day_of_week.hardware() != ((elapsed + 6) % 7) as u8 {
            return Err(Error::InvalidDayOfWeek(day_of_week as u8));
        }
        if hour > 23 {
            return Err(Error::InvalidHour);
        }
        if minute > 59 {
            return Err(Error::InvalidMinute);
        }
        if second > 59 {
            return Err(Error::InvalidSecond);
        }
        if usecond != 0 {
            return Err(Error::InvalidMicrosecond);
        }
        Ok(Self {
            year,
            month,
            day,
            day_of_week,
            hour,
            minute,
            second,
        })
    }
    /// Year in 2000..=2099.
    pub const fn year(&self) -> u16 {
        self.year
    }
    /// Month in 1..=12.
    pub const fn month(&self) -> u8 {
        self.month
    }
    /// Day in the validated month.
    pub const fn day(&self) -> u8 {
        self.day
    }
    /// Gregorian weekday.
    pub const fn day_of_week(&self) -> DayOfWeek {
        self.day_of_week
    }
    /// Hour in 0..=23.
    pub const fn hour(&self) -> u8 {
        self.hour
    }
    /// Minute in 0..=59.
    pub const fn minute(&self) -> u8 {
        self.minute
    }
    /// Second in 0..=59.
    pub const fn second(&self) -> u8 {
        self.second
    }
    /// Always zero; no subsecond precision is promised.
    pub const fn microsecond(&self) -> u32 {
        0
    }
}
