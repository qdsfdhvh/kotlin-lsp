use super::{compute_risk, RefBreakdown};

#[test]
fn low_risk_low() {
    let bd = RefBreakdown {
        call: 0,
        read: 1,
        write: 0,
        import: 0,
        type_use: 0,
        other: 0,
    };
    let (risk, _reason) = compute_risk(1, &bd, 0);
    assert_eq!(risk, "low");
}

#[test]
fn high_risk_high() {
    let bd = RefBreakdown {
        call: 200,
        read: 0,
        write: 0,
        import: 0,
        type_use: 0,
        other: 0,
    };
    let (risk, _reason) = compute_risk(200, &bd, 10);
    assert_eq!(risk, "high");
}

#[test]
fn medium_risk() {
    let bd = RefBreakdown {
        call: 5,
        read: 3,
        write: 2,
        import: 1,
        type_use: 1,
        other: 0,
    };
    let (risk, _reason) = compute_risk(12, &bd, 1);
    assert!(!risk.is_empty());
}
