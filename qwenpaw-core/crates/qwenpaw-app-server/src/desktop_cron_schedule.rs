//! Native time calculations for the original APScheduler-backed job contract.

use chrono::DateTime;
use chrono::Datelike as _;
use chrono::Duration;
use chrono::FixedOffset;
use chrono::LocalResult;
use chrono::NaiveDate;
use chrono::NaiveDateTime;
use chrono::Offset as _;
use chrono::TimeZone as _;
use chrono::Utc;
use chrono_tz::Tz;

use super::ApiError;
use super::ScheduleSpec;
use super::unprocessable;

pub(super) enum Schedule {
    Cron {
        zone: Tz,
        fields: [Field; 5],
    },
    Once {
        start: DateTime<Utc>,
        interval: Option<Duration>,
        end: Option<DateTime<Utc>>,
    },
}

pub(super) struct Field {
    values: Vec<u32>,
    last_day: bool,
}

impl Field {
    fn contains(&self, value: u32) -> bool {
        self.values.contains(&value)
    }

    fn parse(value: &str, min: u32, max: u32, names: &[&str], day: bool) -> Result<Self, ApiError> {
        let mut values = Vec::new();
        let mut last_day = false;
        for part in value.split(',') {
            let part = part.to_ascii_lowercase();
            if day && part == "last" {
                last_day = true;
                continue;
            }
            let (base, step_text) = part
                .split_once('/')
                .map_or((part.as_str(), None), |(a, b)| (a, Some(b)));
            let uses_names =
                !names.is_empty() && base.bytes().any(|value| value.is_ascii_alphabetic());
            let number = |text: &str| -> Result<u32, ApiError> {
                names
                    .iter()
                    .position(|name| *name == text)
                    .and_then(|index| u32::try_from(index).ok())
                    .map(|index| index + min)
                    .or_else(|| text.parse().ok())
                    .ok_or_else(|| unprocessable("cron field is invalid"))
            };
            let (start, end) = if base == "*" {
                (min, max)
            } else if let Some((start, end)) = base.split_once('-') {
                (number(start)?, number(end)?)
            } else {
                let start = number(base)?;
                (
                    start,
                    if step_text.is_some() && !uses_names {
                        max
                    } else {
                        start
                    },
                )
            };
            // APScheduler 3 named month/weekday ranges do not apply a step.
            // Numeric weekdays have already been normalized to names by CoPaw.
            let step = if uses_names {
                1
            } else {
                step_text
                    .map_or(Ok(1), str::parse::<u32>)
                    .map_err(|_| unprocessable("cron step is invalid"))?
            };
            if start < min
                || end > max
                || start > end
                || step == 0
                || (!uses_names && step_text.is_some() && step > end - start)
            {
                return Err(unprocessable("cron field range or step is invalid"));
            }
            values.extend((start..=end).filter(|value| (value - start) % step == 0));
        }
        values.sort_unstable();
        values.dedup();
        if values.is_empty() && !last_day {
            return Err(unprocessable("cron field is empty"));
        }
        Ok(Self { values, last_day })
    }
}

impl Schedule {
    pub(super) fn parse(spec: &ScheduleSpec) -> Result<Self, ApiError> {
        let zone = timezone(&spec.timezone)?;
        if spec.kind == "cron" {
            let fields = spec
                .cron
                .as_deref()
                .unwrap_or_default()
                .split_whitespace()
                .collect::<Vec<_>>();
            if fields.len() != 5 {
                return Err(unprocessable("cron must have 5 fields"));
            }
            return Ok(Self::Cron {
                zone,
                fields: [
                    Field::parse(fields[0], 0, 59, &[], false)?,
                    Field::parse(fields[1], 0, 23, &[], false)?,
                    Field::parse(fields[2], 1, 31, &[], true)?,
                    Field::parse(
                        fields[3],
                        1,
                        12,
                        &[
                            "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct",
                            "nov", "dec",
                        ],
                        false,
                    )?,
                    Field::parse(
                        fields[4],
                        0,
                        6,
                        &["mon", "tue", "wed", "thu", "fri", "sat", "sun"],
                        false,
                    )?,
                ],
            });
        }
        let start = datetime(spec.run_at.as_deref().unwrap_or_default(), zone)?;
        let interval = spec
            .repeat_every_days
            .map(|days| Duration::days(i64::from(days)));
        let end = match (interval, spec.repeat_end_type.as_deref()) {
            (Some(_), Some("until")) => Some(datetime(
                spec.repeat_until.as_deref().unwrap_or_default(),
                zone,
            )?),
            (Some(interval), Some("count")) => {
                let count = spec
                    .repeat_count
                    .ok_or_else(|| unprocessable("repeat_count is missing"))?;
                let seconds = interval
                    .num_seconds()
                    .checked_mul(i64::from(count.saturating_sub(1)))
                    .ok_or_else(|| {
                        unprocessable("repeat_count exceeds the supported datetime range")
                    })?;
                Some(
                    start
                        .checked_add_signed(Duration::try_seconds(seconds).ok_or_else(|| {
                            unprocessable("repeat_count exceeds the supported datetime range")
                        })?)
                        .filter(|value| value.year() <= 9999)
                        .ok_or_else(|| {
                            unprocessable("repeat_count exceeds the supported datetime range")
                        })?,
                )
            }
            _ => None,
        };
        Ok(Self::Once {
            start,
            interval,
            end,
        })
    }

    /// First registration preserves an overdue one-shot for the misfire gate.
    pub(super) fn initial(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        if let Self::Once {
            start,
            interval: None,
            ..
        } = self
        {
            Some(*start)
        } else {
            self.next(now, true)
        }
    }

    pub(super) fn next(&self, from: DateTime<Utc>, inclusive: bool) -> Option<DateTime<Utc>> {
        let from = if inclusive {
            from
        } else {
            from.checked_add_signed(Duration::nanoseconds(1))?
        };
        match self {
            Self::Once {
                start,
                interval,
                end,
            } => {
                let next = if from <= *start {
                    *start
                } else {
                    let interval = (*interval)?;
                    let elapsed = from.signed_duration_since(*start);
                    let mut count = elapsed.num_seconds() / interval.num_seconds();
                    let candidate = start.checked_add_signed(Duration::try_seconds(
                        count.checked_mul(interval.num_seconds())?,
                    )?)?;
                    if candidate < from {
                        count += 1;
                    }
                    start.checked_add_signed(Duration::try_seconds(
                        count.checked_mul(interval.num_seconds())?,
                    )?)?
                };
                (next.year() <= 9999 && end.is_none_or(|end| next <= end)).then_some(next)
            }
            Self::Cron { zone, fields } => cron_occurrence(*zone, fields, from, true),
        }
    }

    pub(super) fn latest_due(
        &self,
        first: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Option<DateTime<Utc>> {
        let latest = match self {
            Self::Once {
                start,
                interval: None,
                ..
            } => *start,
            Self::Once {
                start,
                interval: Some(interval),
                end,
            } => {
                let until = end.map_or(now, |end| end.min(now));
                if until < *start {
                    return None;
                }
                let count =
                    until.signed_duration_since(*start).num_seconds() / interval.num_seconds();
                start.checked_add_signed(Duration::try_seconds(
                    count.checked_mul(interval.num_seconds())?,
                )?)?
            }
            Self::Cron { zone, fields } => cron_occurrence(*zone, fields, now, false)?,
        };
        (latest >= first && latest <= now).then_some(latest)
    }
}

fn cron_occurrence(
    zone: Tz,
    fields: &[Field; 5],
    from: DateTime<Utc>,
    forward: bool,
) -> Option<DateTime<Utc>> {
    let local = from.with_timezone(&zone);
    let [minute, hour, day, month, weekday] = fields;
    // A Gregorian cycle covers every possible combination, including leap days.
    // Iterate allowed months/days, not seconds or every missed cron occurrence.
    for offset in 0..=400 {
        let year = local.year() + if forward { offset } else { -offset };
        if !(1..=9999).contains(&year) {
            return None;
        }
        for index in 0..month.values.len() {
            let month = month.values[if forward {
                index
            } else {
                month.values.len() - index - 1
            }];
            for day_index in 1..=31 {
                let date_day = if forward { day_index } else { 32 - day_index };
                let Some(date) = NaiveDate::from_ymd_opt(year, month, date_day) else {
                    continue;
                };
                if (if forward {
                    date < local.date_naive()
                } else {
                    date > local.date_naive()
                }) || !weekday.contains(date.weekday().num_days_from_monday())
                {
                    continue;
                }
                let last = date.succ_opt().is_none_or(|next| next.month() != month);
                if !(day.contains(date_day) || day.last_day && last) {
                    continue;
                }
                let mut earliest = None;
                for &hour in &hour.values {
                    for &minute in &minute.values {
                        let wall = date.and_hms_opt(hour, minute, 0)?;
                        for candidate in local_candidates(wall, zone).into_iter().flatten() {
                            let candidate = candidate.with_timezone(&Utc);
                            if (if forward {
                                candidate >= from
                            } else {
                                candidate <= from
                            }) && earliest.is_none_or(|value| {
                                if forward {
                                    candidate < value
                                } else {
                                    candidate > value
                                }
                            }) {
                                earliest = Some(candidate);
                            }
                        }
                    }
                }
                if earliest.is_some() {
                    return earliest;
                }
            }
        }
    }
    None
}

fn local_candidates(wall: NaiveDateTime, zone: Tz) -> [Option<DateTime<FixedOffset>>; 2] {
    match zone.from_local_datetime(&wall) {
        LocalResult::Single(value) => [Some(value.fixed_offset()), None],
        LocalResult::Ambiguous(first, second) => {
            [Some(first.fixed_offset()), Some(second.fixed_offset())]
        }
        LocalResult::None => {
            // ZoneInfo/APS 3 attaches the pre-gap offset to nonexistent wall times.
            let before = chrono_tz::GapInfo::new(&wall, &zone).and_then(|gap| gap.begin);
            [
                before.and_then(|(_, offset)| wall.and_local_timezone(offset.fix()).single()),
                None,
            ]
        }
    }
}

pub(super) fn timezone(value: &str) -> Result<Tz, ApiError> {
    value
        .parse()
        .map_err(|_| unprocessable("schedule timezone is invalid"))
}

pub(super) fn datetime(value: &str, zone: Tz) -> Result<DateTime<Utc>, ApiError> {
    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return (1..=9999)
            .contains(&value.year())
            .then(|| value.with_timezone(&Utc))
            .ok_or_else(|| unprocessable("scheduled datetime is invalid"));
    }
    let wall = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
        .map_err(|_| unprocessable("scheduled datetime is invalid"))?;
    local_candidates(wall, zone)[0]
        .filter(|value| (1..=9999).contains(&value.year()))
        .map(|value| value.with_timezone(&Utc))
        .ok_or_else(|| unprocessable("scheduled datetime is invalid"))
}

#[cfg(test)]
#[path = "desktop_cron_schedule_tests.rs"]
mod tests;
