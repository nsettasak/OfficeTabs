use officetabs::model::*;
#[test]
fn activation_relayout_restores_tab_band_without_accumulating_inset() {
    let original = Bounds {
        x: 0,
        y: 200,
        width: 1200,
        height: 700,
    };
    let applied = Bounds {
        x: 0,
        y: 244,
        width: 1200,
        height: 656,
    };
    assert_eq!(
        reserve_workspace(original, applied, 44),
        Some((original, applied))
    );
    let mut current = applied;
    for _ in 0..100 {
        let (base, next) = reserve_workspace(current, applied, 44).unwrap();
        assert_eq!(base, original);
        assert_eq!(next, applied);
        current = next;
    }
}
#[test]
fn workspace_resize_preserves_band_and_rejects_collapsed_host() {
    let applied = Bounds {
        x: 0,
        y: 244,
        width: 1200,
        height: 656,
    };
    let current = Bounds {
        height: 500,
        ..applied
    };
    let (base, next) = reserve_workspace(current, applied, 44).unwrap();
    assert_eq!(base.y, 200);
    assert_eq!(base.height, 544);
    assert_eq!(next, current);
    assert!(reserve_workspace(
        Bounds {
            height: 0,
            ..current
        },
        applied,
        44
    )
    .is_none());
}
fn doc(id: usize, frame: usize) -> Document {
    Document {
        id,
        name: format!("Doc {id}"),
        saved: false,
        active: id == 1,
        frame,
    }
}
#[test]
fn empty_start_has_no_tabs() {
    let m = Model::default();
    assert!(m.view(10).is_empty());
}
#[test]
fn order_survives_snapshots() {
    let mut m = Model::default();
    m.snapshot(vec![doc(1, 10), doc(2, 20), doc(3, 30)]);
    m.reorder(3, Some(1));
    m.snapshot(vec![doc(2, 20), doc(1, 10), doc(3, 30)]);
    assert_eq!(
        m.documents.iter().map(|d| d.id).collect::<Vec<_>>(),
        vec![3, 1, 2]
    );
}
#[test]
fn omissions_preserve_independent_ownership() {
    let mut m = Model::default();
    m.snapshot(vec![doc(1, 10), doc(2, 20)]);
    m.detach(20);
    m.snapshot(vec![doc(1, 10)]);
    assert!(m.independent.contains(&20));
    m.snapshot(vec![doc(1, 10), doc(3, 20)]);
    assert_eq!(m.view(20)[0].id, 3);
    assert_eq!(m.view(10).len(), 1);
}
#[test]
fn dock_restores_group_in_one_transition() {
    let mut m = Model::default();
    m.snapshot(vec![doc(1, 10), doc(2, 20)]);
    m.detach(20);
    m.dock(20, 10);
    assert!(m.independent.is_empty());
    assert_eq!(m.view(10).len(), 2);
    assert_eq!(m.group_frame, Some(10));
}
#[test]
fn primary_secondary_directions_are_symmetric() {
    for (a, b) in [(10, 20), (20, 10)] {
        let mut m = Model::default();
        m.detach(a);
        m.detach(b);
        m.dock(a, b);
        assert!(m.independent.is_empty());
        assert_eq!(m.group_frame, Some(b));
    }
}
#[test]
fn closing_independent_does_not_remove_other() {
    let mut m = Model::default();
    m.detach(10);
    m.detach(20);
    m.prune_frames(|f| f != 10);
    assert_eq!(m.independent.into_iter().collect::<Vec<_>>(), vec![20]);
}
#[test]
fn absent_tab_is_restored_at_old_position() {
    let mut m = Model::default();
    m.snapshot(vec![doc(1, 10), doc(2, 20)]);
    m.reorder(2, Some(1));
    m.snapshot(vec![]);
    m.snapshot(vec![doc(1, 10), doc(2, 20)]);
    assert_eq!(m.documents[0].id, 2);
}
#[test]
fn invalid_reorder_is_ignored() {
    let mut m = Model::default();
    m.snapshot(vec![doc(1, 10), doc(2, 20)]);
    m.reorder(1, Some(99));
    m.reorder(1, Some(1));
    assert_eq!(m.documents[0].id, 1);
}
#[test]
fn new_tabs_append() {
    let mut m = Model::default();
    m.snapshot(vec![doc(1, 10), doc(2, 20)]);
    m.reorder(2, Some(1));
    m.snapshot(vec![doc(3, 30), doc(1, 10), doc(2, 20)]);
    assert_eq!(
        m.documents.iter().map(|d| d.id).collect::<Vec<_>>(),
        vec![2, 1, 3]
    );
}
#[test]
fn click_is_not_detach() {
    let mut d = Drag::default();
    d.start(1, 10, 10);
    d.update(11, 10, 4);
    assert_eq!(d.finish(11, 10, true, None, None), DropAction::Activate(1));
}
#[test]
fn in_strip_drag_reorders() {
    let mut d = Drag::default();
    d.start(1, 10, 10);
    d.update(300, 10, 4);
    assert_eq!(
        d.finish(300, 10, true, Some(2), None),
        DropAction::Reorder(1, Some(2))
    );
}
#[test]
fn cross_monitor_drag_commits_same_gesture() {
    let mut d = Drag::default();
    d.start(1, -1000, 30);
    d.update(800, 30, 4);
    assert_eq!(
        d.finish(800, 30, false, None, Some(20)),
        DropAction::Dock(1, 20)
    );
}
#[test]
fn outside_target_detaches() {
    let mut d = Drag::default();
    d.start(1, 0, 0);
    d.update(-2000, 300, 4);
    assert_eq!(
        d.finish(-2000, 300, false, None, None),
        DropAction::Detach(1, -2000, 300)
    );
}
#[test]
fn escape_has_no_action() {
    let mut d = Drag::default();
    d.start(1, 0, 0);
    d.update(300, 300, 4);
    d.cancel();
    assert_eq!(d.finish(300, 300, false, None, None), DropAction::Cancel);
}
#[test]
fn adaptive_widths_do_not_expand_short_tabs() {
    assert_eq!(tab_widths(&[88, 300], 500), vec![88, 300]);
    assert_eq!(tab_widths(&[88, 300], 200), vec![88, 112]);
}
#[test]
fn overflow_remains_scrollable() {
    assert_eq!(tab_widths(&[100; 10], 400), vec![72; 10]);
}
#[test]
fn animation_is_frame_rate_independent() {
    let whole = ease(0.0, 500.0, 0.08);
    let mut parts = 0.0;
    for _ in 0..8 {
        parts = ease(parts, 500.0, 0.01);
    }
    assert!((whole - parts).abs() < 0.01);
}
#[test]
fn animation_catches_up_after_blocked_host() {
    assert_eq!(ease(0.0, 100.0, 1.0), 100.0);
}
#[test]
fn unknown_progress_is_not_zero_percent() {
    assert_eq!(Progress::default().percent(), None);
}
#[test]
fn msi_progress_counts_actual_ticks() {
    let mut p = Progress::default();
    p.message(0, 1000, 0, 0);
    p.message(2, 250, 0, 0);
    assert_eq!(p.percent(), Some(25));
    p.message(3, 1000, 0, 0);
    assert_eq!(p.percent(), Some(12));
}
#[test]
fn msi_actiondata_increments_only_when_requested() {
    let mut p = Progress::default();
    p.message(0, 100, 0, 0);
    p.action();
    assert_eq!(p.percent(), Some(0));
    p.message(1, 20, 1, 0);
    p.action();
    assert_eq!(p.percent(), Some(20));
}
#[test]
fn rollback_progress_moves_backward() {
    let mut p = Progress::default();
    p.message(0, 100, 1, 0);
    assert_eq!(p.percent(), Some(100));
    p.message(2, 30, 0, 0);
    assert_eq!(p.percent(), Some(70));
}
#[test]
fn script_preparation_is_distinct() {
    let mut p = Progress::default();
    p.message(0, 100, 0, 1);
    assert!(p.script);
    p.message(0, 100, 0, 0);
    assert!(!p.script);
}
#[test]
fn progress_never_overflows() {
    let mut p = Progress::default();
    p.message(0, i32::MAX, 0, 0);
    p.message(2, i32::MAX, 0, 0);
    p.message(2, i32::MAX, 0, 0);
    assert_eq!(p.percent(), Some(100));
}
#[test]
fn bounds_support_negative_monitors() {
    let b = Bounds {
        x: -1920,
        y: -200,
        width: 1920,
        height: 1080,
    };
    assert!(b.contains(-900, 0));
    assert!(!b.contains(0, 0));
}

#[test]
fn many_tabs_keep_the_remembered_order() {
    let docs = |n: usize| -> Vec<Document> {
        (0..n)
            .map(|i| Document {
                id: i,
                name: format!("Document {i}.docx"),
                saved: false,
                active: i == 0,
                frame: 1,
            })
            .collect()
    };
    let mut model = Model::default();
    model.snapshot(docs(60));
    assert_eq!(model.view(1).len(), 60);
    // Move the last tab to the front, then re-snapshot as Office would.
    model.reorder(59, Some(0));
    let order: Vec<_> = model.view(1).iter().map(|d| d.id).collect();
    assert_eq!(order[0], 59);
    assert_eq!(order.len(), 60);
    model.snapshot(docs(60));
    assert_eq!(model.view(1)[0].id, 59);
    // Closing a tab must not resurrect it on the next snapshot.
    model.snapshot(docs(59));
    assert!(model.view(1).iter().all(|d| d.id != 59));
}
#[test]
fn reorder_to_an_unknown_target_is_ignored() {
    let mut model = Model::default();
    model.snapshot(vec![Document {
        id: 1,
        name: "a".into(),
        saved: false,
        active: true,
        frame: 1,
    }]);
    model.reorder(1, Some(99));
    assert_eq!(model.view(1)[0].id, 1);
    model.reorder(99, None);
    assert_eq!(model.view(1)[0].id, 1);
}
#[test]
fn detached_document_leaves_the_group_view() {
    let mut model = Model::default();
    model.snapshot(vec![
        Document {
            id: 1,
            name: "a".into(),
            saved: false,
            active: true,
            frame: 10,
        },
        Document {
            id: 2,
            name: "b".into(),
            saved: false,
            active: false,
            frame: 10,
        },
    ]);
    model.detach(10);
    assert!(model.view(10).iter().all(|d| d.frame == 10));
    model.snapshot(vec![
        Document {
            id: 1,
            name: "a".into(),
            saved: false,
            active: true,
            frame: 10,
        },
        Document {
            id: 2,
            name: "b".into(),
            saved: false,
            active: false,
            frame: 11,
        },
    ]);
    // Frame 11 is not detached, so it joins the shared group.
    assert_eq!(model.view(11).len(), 1);
    model.dock(10, 11);
    assert_eq!(model.view(11).len(), 2);
}
#[test]
fn narrow_host_shrinks_tabs_before_dropping_them() {
    // 20 tabs of 240px cannot fit 1600px, so every tab narrows evenly.
    let preferred = vec![240; 20];
    assert_eq!(tab_widths(&preferred, 1600), vec![80; 20]);
    // Past the clickable minimum the band stops shrinking and scrolls instead.
    let crowded = vec![240; 40];
    assert_eq!(tab_widths(&crowded, 1600), vec![72; 40]);
    // Plenty of room: preferred widths are kept exactly.
    assert_eq!(tab_widths(&preferred, 8000), preferred);
}
