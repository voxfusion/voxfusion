//! How download sizes and the time a download has left are written.

const BYTES_PER_MB: f64 = 1024. * 1024.;

/// `value` with one decimal, halves rounded up.
fn one_decimal(value: f64) -> String {
    let tenths = value * 10.;

    // The product is rounded to the nearest number a float can hold. If that
    // lands exactly on a half, what the rounding dropped tells on which side
    // of the half the value really is.
    let dropped = value.mul_add(10., -tenths);
    let rounded = if tenths.fract() == 0.5 && dropped < 0. {
        tenths.floor()
    } else {
        (tenths + 0.5).floor()
    } as u64;

    format!("{}.{}", rounded / 10, rounded % 10)
}

/// A number of bytes in megabytes, without the unit: `312.0`.
pub fn format_mb(bytes: f64) -> String {
    one_decimal(bytes / BYTES_PER_MB)
}

/// The time left as `m:ss`, or `h:mm:ss` from an hour on.
pub fn format_eta(eta_seconds: f64) -> String {
    let total = eta_seconds.round().max(0.) as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn megabytes_have_one_decimal() {
        assert_eq!(format_mb(0.), "0.0");
        assert_eq!(format_mb(327_180_000.), "312.0");
        assert_eq!(format_mb(779_000_000.), "742.9");
        assert_eq!(format_mb(1024. * 1024. * 47.5), "47.5");
    }

    #[test]
    fn halves_round_up_and_near_halves_do_not() {
        // A quarter of a megabyte is exactly 0.25.
        assert_eq!(format_mb(262_144.), "0.3");
        assert_eq!(format_mb(786_432.), "0.8");

        // The floats nearest to these are a little off the half they look
        // to be on.
        assert_eq!(one_decimal(0.15), "0.1");
        assert_eq!(one_decimal(0.35), "0.3");
        assert_eq!(one_decimal(1.45), "1.4");
        assert_eq!(one_decimal(0.95), "0.9");
        assert_eq!(one_decimal(0.05), "0.1");
    }

    #[test]
    fn time_left_gains_hours_only_when_needed() {
        assert_eq!(format_eta(0.), "0:00");
        assert_eq!(format_eta(0.4), "0:00");
        assert_eq!(format_eta(0.5), "0:01");
        assert_eq!(format_eta(59.), "0:59");
        assert_eq!(format_eta(92.2), "1:32");
        assert_eq!(format_eta(3600.), "1:00:00");
        assert_eq!(format_eta(52_428.), "14:33:48");
        assert_eq!(format_eta(-3.), "0:00");
    }
}
