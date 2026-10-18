use glorious_core_rs::{
    protocol::*,
    state::{self, State},
    tables::*,
};
// Expected bytes are hand-derived from named JS functions. Omitted tail bytes are zero.
fn expected(bytes: &[u8]) -> Report {
    let mut r = [0; 64];
    r[..bytes.len()].copy_from_slice(bytes);
    r
}
#[test]
fn firmware() {
    // PrepareGetFWVersion
    assert_eq!(get_fw(240).unwrap(), expected(&[240, 0, 0, 0, 0, 0, 0, 15]));
    assert_eq!(
        get_fw(128).unwrap(),
        expected(&[128, 0, 0, 0, 0, 0, 0, 127])
    );
    assert_eq!(get_fw(174).unwrap(), expected(&[174, 0, 0, 0, 0, 0, 0, 81]));
    assert_eq!(parse_fw(&[240, 0x34, 0x12]).unwrap(), "KBv1234");
    assert!(parse_fw(&[240]).is_err());
    assert!(get_fw(0).is_err());
}
#[test]
fn profiles() {
    // PrepareGetProfileIndex, PrepareSetProfileIndex, ProfileAndLayerIndexToFWValue
    assert_eq!(get_profile(), expected(&[133, 0, 0, 0, 0, 0, 0, 122]));
    assert_eq!(
        set_profile(2, 1).unwrap(),
        expected(&[5, 2, 1, 0, 0, 0, 0, 247])
    );
    assert_eq!(parse_profile(&[133, 2, 1]).unwrap(), (2, 1));
    assert!(parse_profile(&[133, 3, 0]).is_err());
    assert!(set_profile(3, 0).is_err());
}
#[test]
fn sleep() {
    // SleepTimeSettings.toGetReportBuffer / toReportBuffer (USB only)
    assert_eq!(get_sleep(), expected(&[146, 0, 0, 0, 0, 0, 0, 109]));
    assert_eq!(
        set_sleep(300),
        expected(&[
            18, 0, 0, 0, 0, 0, 0, 237, 0, 0, 0, 0, 0, 0, 0, 0, 44, 1, 255, 255
        ])
    );
    assert_eq!(
        set_sleep(0),
        expected(&[
            18, 0, 0, 0, 0, 0, 0, 237, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255
        ])
    );
    let mut r = [0; 13];
    r[0] = 146;
    r[9] = 44;
    r[10] = 1;
    assert_eq!(parse_sleep(&r).unwrap(), Some(300));
    r[9] = 255;
    r[10] = 255;
    assert_eq!(parse_sleep(&r).unwrap(), None);
    assert!(parse_sleep(&[146]).is_err());
}
#[test]
fn performance() {
    // PrepareSetReportRate / PrepareDebounceBuffer
    assert_eq!(
        report_rate(2, 1000).unwrap(),
        expected(&[4, 2, 3, 0, 0, 0, 0, 246])
    );
    assert_eq!(
        debounce(13).unwrap(),
        expected(&[17, 0, 3, 0, 0, 0, 0, 235])
    );
    assert!(report_rate(0, 999).is_err());
    assert!(debounce(1276).is_err());
    for (hz, v) in [
        (8000, 0),
        (4000, 1),
        (2000, 2),
        (1000, 3),
        (500, 4),
        (250, 5),
        (125, 6),
    ] {
        assert_eq!(report_rate(0, hz).unwrap()[2], v);
    }
}
#[test]
fn dangerous() {
    // PrepareReset / PrepareSwitchesCalibration
    assert_eq!(reset(), expected(&[2, 0, 0, 0, 0, 0, 0, 253]));
    assert_eq!(
        calibration(1).unwrap(),
        expected(&[28, 1, 0, 0, 0, 0, 0, 226])
    );
    assert_eq!(
        calibration(2).unwrap(),
        expected(&[28, 0, 0, 0, 0, 0, 0, 227])
    );
    assert_eq!(
        calibration(3).unwrap(),
        expected(&[30, 1, 0, 0, 0, 0, 0, 224])
    );
    assert_eq!(
        calibration(4).unwrap(),
        expected(&[30, 0, 0, 0, 0, 0, 0, 225])
    );
    assert!(calibration(0).is_err());
}
#[test]
fn live() {
    // PrepareVisualization / VisualizationToPercent
    assert_eq!(visualization(true), expected(&[27, 1, 0, 0, 0, 0, 0, 227]));
    assert_eq!(visualization(false), expected(&[27, 0, 0, 0, 0, 0, 0, 228]));
    assert_eq!(parse_visualization(&[5, 27, 20]), Some(0.5));
    assert_eq!(parse_visualization(&[5, 27, 255]), Some(1.));
    assert_eq!(parse_visualization(&[5, 1, 20]), None);
    assert_eq!(parse_visualization(&[5]), None);
}
#[test]
fn magnetism() {
    // PrepareSetCustomActuationMode / PrepareActuationPointAndRapidTriggerMagnetismReport
    assert_eq!(custom_actuation(), expected(&[29, 3, 0, 0, 0, 0, 0, 223]));
    let a = Actuation {
        press: 2.,
        rapid: true,
        rapid_press: 0.2,
        rapid_release: 0.4,
        deadzone: Some(0.3),
    };
    assert_eq!(
        actuation(1, 2, None, &a).unwrap(),
        expected(&[
            26, 33, 0, 128, 19, 1, 0, 48, 19, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2
        ])
    );
    // Per-key path: handler sends no deadzone, so firmware gets 40.
    let a = Actuation {
        deadzone: None,
        ..Actuation::default()
    };
    assert_eq!(
        actuation(0, 0, Some(9), &a).unwrap(),
        expected(&[
            26, 0, 0, 0, 19, 0, 9, 201, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 40
        ])
    );
    assert!(
        actuation(
            0,
            0,
            None,
            &Actuation {
                press: f32::NAN,
                ..a
            }
        )
        .is_err()
    );
}
#[test]
fn keystrokes() {
    // PrepareSetKeystroke / PrepareKeystrokeClear + UpdateKeystrokeClear / #setComplexKeystroke
    assert_eq!(
        keystroke(9, 1, 2, 3, [0, 224, 4, 0]).unwrap(),
        expected(&[19, 1, 9, 2, 3, 0, 0, 221, 0, 224, 4, 0])
    );
    assert_eq!(
        keystroke_clear(1, 2, 9).unwrap(),
        expected(&[26, 33, 0, 0, 0, 0, 9, 187])
    );
    let r = complex_keystroke(0, 0, 9, [[0, 0, 4, 0], [0; 4], [0; 4], [0; 4]]).unwrap();
    assert_eq!(r[0], expected(&[19, 0, 9, 0, 0, 0, 0, 227, 0, 0, 4, 0]));
    assert_eq!(r[3], expected(&[19, 0, 9, 0, 3, 0, 0, 224]));
    assert!(keystroke(0, 0, 0, 4, [0; 4]).is_err());
}
#[test]
fn advanced() {
    // PrepareToggleMagnetismReport / PrepareModTapMagnetismReport / PrepareDynamicKeyStrokeMagnetismReport + CreateDKSModes
    assert_eq!(
        toggle(1, 2, 9, 1).unwrap(),
        expected(&[26, 33, 0, 5, 0, 0, 9, 182])
    );
    assert_eq!(
        mod_tap(1, 2, 9, 201).unwrap(),
        expected(&[26, 33, 0, 3, 0, 0, 9, 184, 0, 0, 0, 0, 0, 0, 0, 0, 21])
    );
    assert_eq!(mod_tap(1, 2, 9, 0).unwrap()[16], 1);
    assert_eq!(mod_tap(1, 2, 9, 2000).unwrap()[16], 100);
    assert_eq!(
        dynamic_keystroke(1, 2, 9, Some(1.), Some(2.), [1, 2, 0, 1]).unwrap(),
        expected(&[26, 33, 0, 2, 0, 0, 9, 185, 0, 0, 0, 9, 1, 8, 0, 64])
    );
    assert_eq!(
        dynamic_keystroke(0, 0, 9, None, None, [0; 4]).unwrap()[11],
        24
    );
    assert!(dynamic_keystroke(0, 0, 0, None, None, [3; 4]).is_err());
}
#[test]
fn lights() {
    // LightingEffect.toUint8Array / MapEffectValue$1 / PrepareLightingBuffers
    let a = Lighting {
        effect: 2,
        speed: 51,
        brightness: 1,
        color_mode: 9,
        kind: 1,
        rgb: [10, 20, 30],
    };
    assert_eq!(
        lighting(&a, false).unwrap(),
        expected(&[7, 6, 9, 1, 25, 10, 20, 30, 147])
    );
    assert_eq!(
        lighting(&a, true).unwrap(),
        expected(&[8, 6, 9, 1, 25, 10, 20, 30, 146])
    );
    assert_eq!(
        (0..=18).map(map_effect).collect::<Vec<_>>(),
        vec![
            1, 2, 6, 3, 4, 5, 7, 8, 0, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18
        ]
    );
    assert!(lighting(&Lighting { speed: 101, ..a }, false).is_err());
}
#[test]
fn color_pages() {
    // PrepareSetPerKeyColor / PrepareSetPerKeyColorMode / MagnetonPerKeyLighting / Paginate
    assert_eq!(
        per_key_color(8, &[1, 2, 3]).unwrap(),
        expected(&[12, 0, 128, 1, 8, 0, 0, 106, 1, 2, 3])
    );
    assert_eq!(
        per_key_mode(2, &[1, 2, 0]).unwrap(),
        expected(&[81, 2, 0, 0, 0, 0, 0, 172, 1, 2])
    );
    let mut keys = vec![KeyLight::default(); 168];
    keys[158] = KeyLight {
        rgb: [10, 20, 30],
        mode: 1,
    };
    let r = per_key_lighting(&keys).unwrap();
    assert_eq!(r.len(), 12);
    assert_eq!(r[0], expected(&[12, 0, 128, 1, 0, 0, 0, 114]));
    let mut badge_color = expected(&[12, 0, 128, 1, 8, 0, 0, 106]);
    badge_color[34..37].copy_from_slice(&[10, 20, 30]);
    assert_eq!(r[8], badge_color);
    let mut badge_mode = expected(&[81, 2, 0, 0, 0, 0, 0, 172]);
    badge_mode[54] = 1;
    assert_eq!(r[11], badge_mode);
    assert!(per_key_mode(0, &[3]).is_err());
    assert!(per_key_color(9, &[]).is_err());
}
#[test]
fn macros() {
    // PrepareMacroBuffer$2 / MacroSequence / GenerateMacroKeyCode
    assert_eq!(
        macro_page(7, 1, true, &[1, 2]).unwrap(),
        expected(&[22, 7, 1, 56, 1, 0, 0, 168, 1, 2])
    );
    let events = [
        MacroEvent {
            key: 4,
            press: true,
            delay: 300,
        },
        MacroEvent {
            key: 4,
            press: false,
            delay: 1,
        },
    ];
    assert_eq!(
        macro_reports(7, 1, &events).unwrap(),
        vec![expected(&[
            22, 7, 0, 56, 1, 0, 0, 169, 1, 0, 4, 128, 44, 18, 4, 1, 0, 0
        ])]
    );
    assert_eq!(macro_keycode(7, 2).unwrap(), [9, 2, 7, 0]);
    assert!(
        macro_reports(
            0,
            1,
            &vec![
                MacroEvent {
                    key: 4,
                    press: true,
                    delay: 1
                };
                139
            ]
        )
        .is_err()
    );
    assert!(macro_keycode(0, 3).is_err());
}
#[test]
fn sequence_and_state() {
    let mut s = State {
        profile: 1,
        layer: 2,
        ..State::default()
    };
    s.current_mut().per_key.insert(9, Actuation::default());
    let r = state::activate_reports(&s).unwrap();
    assert_eq!(
        r.iter().map(|r| r[0]).collect::<Vec<_>>(),
        [5, 29, 26, 26, 7, 8, 12, 12, 12, 12, 12, 12, 12, 12, 12, 81, 81, 81]
    );
    assert_eq!(r[0], expected(&[5, 1, 2, 0, 0, 0, 0, 247]));
    assert_eq!(r[2][1], 33);
    let json = serde_json::to_string(&s).unwrap();
    let copy: State = serde_json::from_str(&json).unwrap();
    copy.validate().unwrap();
    assert_eq!(copy.profiles.len(), 3);
    assert_eq!(copy.current().colors.len(), 168);
    s.layer = 3;
    assert!(s.validate().is_err());
}
#[test]
fn tables() {
    assert_eq!(
        PHYSICAL_KEYS.iter().find(|(n, _)| *n == "KeyA").unwrap().1,
        9
    );
    assert_eq!(KEYCODES.iter().find(|(n, _)| *n == "KeyA").unwrap().1, 4);
    assert_eq!(EFFECTS.len(), 19);
}
#[test]
fn wired_battery() {
    // PrepareGetBatteryStateBuffer / ParseBatteryState, USB branch
    assert_eq!(get_battery(), expected(&[131, 0, 0, 0, 0, 0, 0, 124]));
    assert_eq!(parse_battery(&[131, 80, 2]).unwrap(), (80, 2));
    assert!(parse_battery(&[131, 101, 2]).is_err());
}
#[test]
fn checksum_wrap() {
    // CalculateChecksum
    assert_eq!(checksum(&[255, 255, 2]), 255);
    assert_eq!(checksum(&[240]), 15);
}
#[test]
fn macro_timeline() {
    // PrepareMacroData: same-time events get delay 1; initial silence is ignored.
    let actions = [
        MacroAction {
            key: 4,
            start: 100,
            duration: 10,
        },
        MacroAction {
            key: 5,
            start: 110,
            duration: 20,
        },
    ];
    assert_eq!(
        macro_data(2, &actions).unwrap(),
        vec![expected(&[
            22, 2, 0, 56, 1, 0, 0, 174, 1, 0, 4, 138, 4, 1, 5, 148, 5, 1, 0, 0
        ])]
    );
    assert!(macro_data(0, &[]).unwrap().is_empty());
    assert!(
        macro_data(
            0,
            &[MacroAction {
                key: 4,
                start: u32::MAX,
                duration: 1
            }]
        )
        .is_err()
    );
}
