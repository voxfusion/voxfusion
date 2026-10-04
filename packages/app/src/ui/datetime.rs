//! Dates and times as the interface shows them, in each interface language.
//! These follow what `toLocaleTimeString` and `toLocaleDateString` produced
//! for the same options in the original app.

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike, Utc};

use crate::i18n::Locale;

/// The current time. Fixture scenarios run on their own clock.
pub fn now() -> DateTime<Local> {
    #[cfg(feature = "fixture")]
    if let Some(now) = crate::fixture::now() {
        return now.with_timezone(&Local);
    }

    Local::now()
}

/// Parses a stored timestamp (RFC 3339) into local time.
pub fn parse(timestamp: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|time| time.with_timezone(&Local))
        .or_else(|| {
            // SQLite's `datetime()` format, which is UTC.
            chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|time| Utc.from_utc_datetime(&time).with_timezone(&Local))
        })
}

/// `toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" })`.
pub fn format_time<Tz: TimeZone>(time: &DateTime<Tz>, locale: Locale) -> String {
    let (hour, minute) = (time.hour(), time.minute());

    match locale {
        Locale::En => {
            let period = if hour < 12 { "AM" } else { "PM" };
            let hour = match hour % 12 {
                0 => 12,
                hour => hour,
            };
            format!("{hour:02}:{minute:02} {period}")
        }
        Locale::Zh => format!("{hour:02}:{minute:02}"),
        _ => format!("{hour:02}:{minute:02}"),
    }
}

const WEEKDAYS: [[&str; 7]; 7] = [
    [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ],
    [
        "понедельник",
        "вторник",
        "среда",
        "четверг",
        "пятница",
        "суббота",
        "воскресенье",
    ],
    [
        "lunes",
        "martes",
        "miércoles",
        "jueves",
        "viernes",
        "sábado",
        "domingo",
    ],
    [
        "星期一",
        "星期二",
        "星期三",
        "星期四",
        "星期五",
        "星期六",
        "星期日",
    ],
    [
        "Montag",
        "Dienstag",
        "Mittwoch",
        "Donnerstag",
        "Freitag",
        "Samstag",
        "Sonntag",
    ],
    [
        "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
    ],
    [
        "lunedì",
        "martedì",
        "mercoledì",
        "giovedì",
        "venerdì",
        "sabato",
        "domenica",
    ],
];

const MONTHS: [[&str; 12]; 7] = [
    [
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
    ],
    [
        "января",
        "февраля",
        "марта",
        "апреля",
        "мая",
        "июня",
        "июля",
        "августа",
        "сентября",
        "октября",
        "ноября",
        "декабря",
    ],
    [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ],
    [
        "1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月",
    ],
    [
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ],
    [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ],
    [
        "gennaio",
        "febbraio",
        "marzo",
        "aprile",
        "maggio",
        "giugno",
        "luglio",
        "agosto",
        "settembre",
        "ottobre",
        "novembre",
        "dicembre",
    ],
];

fn locale_index(locale: Locale) -> usize {
    Locale::ALL
        .iter()
        .position(|candidate| *candidate == locale)
        .unwrap_or(0)
}

/// `toLocaleDateString(locale, { weekday: "long", month: "long", day: "numeric" })`.
pub fn format_long_date<Tz: TimeZone>(time: &DateTime<Tz>, locale: Locale) -> String {
    let index = locale_index(locale);
    let weekday = WEEKDAYS[index][time.weekday().num_days_from_monday() as usize];
    let month = MONTHS[index][time.month0() as usize];
    let day = time.day();

    match locale {
        Locale::En => format!("{weekday}, {month} {day}"),
        Locale::Ru => format!("{weekday}, {day} {month}"),
        Locale::Es => format!("{weekday}, {day} de {month}"),
        Locale::Zh => format!("{month}{day}日{weekday}"),
        Locale::De => format!("{weekday}, {day}. {month}"),
        Locale::Fr => format!("{weekday} {day} {month}"),
        Locale::It => format!("{weekday} {day} {month}"),
    }
}

/// Whether two times fall on the same calendar day.
pub fn same_day<A: TimeZone, B: TimeZone>(a: &DateTime<A>, b: &DateTime<B>) -> bool {
    a.year() == b.year() && a.ordinal() == b.ordinal()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 30, hour, minute, 0).unwrap()
    }

    #[test]
    fn english_times_use_a_twelve_hour_clock() {
        assert_eq!(format_time(&at(0, 5), Locale::En), "12:05 AM");
        assert_eq!(format_time(&at(11, 42), Locale::En), "11:42 AM");
        assert_eq!(format_time(&at(12, 0), Locale::En), "12:00 PM");
        assert_eq!(format_time(&at(17, 20), Locale::En), "05:20 PM");
    }

    #[test]
    fn other_languages_use_a_twenty_four_hour_clock() {
        assert_eq!(format_time(&at(17, 20), Locale::Ru), "17:20");
        assert_eq!(format_time(&at(8, 3), Locale::De), "08:03");
    }

    #[test]
    fn long_dates_follow_each_language() {
        let date = at(14, 48);

        assert_eq!(
            format_long_date(&date, Locale::En),
            "Wednesday, September 30"
        );
        assert_eq!(format_long_date(&date, Locale::Ru), "среда, 30 сентября");
        assert_eq!(
            format_long_date(&date, Locale::Es),
            "miércoles, 30 de septiembre"
        );
        assert_eq!(format_long_date(&date, Locale::Zh), "9月30日星期三");
        assert_eq!(
            format_long_date(&date, Locale::De),
            "Mittwoch, 30. September"
        );
        assert_eq!(format_long_date(&date, Locale::Fr), "mercredi 30 septembre");
        assert_eq!(
            format_long_date(&date, Locale::It),
            "mercoledì 30 settembre"
        );
    }
}
