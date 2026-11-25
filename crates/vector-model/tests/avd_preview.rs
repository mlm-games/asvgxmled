use vector_model::{ColorSpec, Node, Paint, apply_animations, parse};

const PREVIEWABLE_AVD: &str = r##"<animated-vector xmlns:android="http://schemas.android.com/apk/res/android"
    xmlns:aapt="http://schemas.android.com/aapt">
    <aapt:attr name="android:drawable">
        <vector
            android:width="128dp"
            android:height="128dp"
            android:viewportWidth="128"
            android:viewportHeight="128">
            <group
                android:name="magesMark"
                android:pivotX="64"
                android:pivotY="64"
                android:scaleX="0.8"
                android:scaleY="0.8">
                <path android:name="flare_tr" android:fillColor="#FFFFFFFF"
                    android:pathData="M88.74,32.8C82.88,32.77 72.89,39.79 67.73,43.75C69.08,44.11 70.43,44.59 71.76,45.2C76.49,41.84 83.36,37.74 86.07,40.01C88.79,42.29 85.9,50 83.4,55.39C84.34,56.94 85.08,58.56 85.62,60.23C89.14,53.37 96.43,37.7 91.67,33.73C90.91,33.09 89.91,32.8 88.74,32.8z" />
                <path android:name="flare_bl" android:fillColor="#FFFFFFFF"
                    android:pathData="M42.78,68.9C39.12,76.16 32.67,90.63 37.21,94.43C41.83,98.28 54.88,89.22 61.09,84.46C59.74,84.14 58.39,83.71 57.06,83.16C52.35,86.48 45.69,90.37 43.03,88.15C40.46,86 42.9,78.99 45.29,73.68C44.25,72.16 43.41,70.55 42.78,68.9z" />
                <path android:name="ring" android:fillColor="#FFFFFFFF"
                    android:pathData="m53.91,45.12c-6.1,0 -11.01,4.91 -11.01,11.01v7.74c0,6.1 4.91,11.01 11.01,11.01h0.89c-4.3,-0.99 -7.49,-4.82 -7.49,-9.43v-3.93c0,-5.36 4.31,-9.67 9.67,-9.67h10.69c4.61,0 8.44,3.19 9.43,7.49V56.13C77.1,50.03 72.19,45.12 66.09,45.12Z" />
                <path android:name="crystal" android:fillColor="#FFFFFFFF"
                    android:pathData="M84.2,75.66L84.2,75.66A20.3,23.32 120,0 1,53.85 81.58L53.85,81.58A20.3,23.32 120,0 1,43.8 52.34L43.8,52.34A20.3,23.32 120,0 1,74.15 46.42L74.15,46.42A20.3,23.32 120,0 1,84.2 75.66z" />
            </group>
        </vector>
    </aapt:attr>

    <target android:name="magesMark">
        <aapt:attr name="android:animation">
            <objectAnimator
                android:propertyName="scaleX"
                android:valueFrom="0.8"
                android:valueTo="0.832"
                android:duration="1000"
                android:repeatCount="999"
                android:repeatMode="reverse" />
            <objectAnimator
                android:propertyName="scaleY"
                android:valueFrom="0.8"
                android:valueTo="0.832"
                android:duration="1000"
                android:repeatCount="999"
                android:repeatMode="reverse" />
        </aapt:attr>
    </target>
</animated-vector>
"##;

#[test]
fn previewable_avd_parses_and_animates() {
    let result = parse(PREVIEWABLE_AVD);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let doc = result.doc.expect("AVD parses");
    assert_eq!(doc.viewport_width, 128.0);
    assert_eq!(doc.animations.len(), 1);
    assert_eq!(doc.animations[0].animators.len(), 2);
    assert_eq!(vector_model::total_duration(&doc), 1000);

    let at_start = apply_animations(&doc, 0);
    let at_half = apply_animations(&doc, 500);
    let value = |d: &vector_model::VectorDoc| match &d.nodes[0] {
        vector_model::Node::Group(g) => g.scale_x,
        _ => panic!("group"),
    };
    assert!((value(&at_start) - 0.8).abs() < 1e-6);
    assert!((value(&at_half) - 0.816).abs() < 1e-6);
}

fn first_path(doc: &vector_model::VectorDoc) -> &vector_model::model::PathNode {
    fn find(nodes: &[Node]) -> Option<&vector_model::model::PathNode> {
        for n in nodes {
            match n {
                Node::Path(p) => return Some(p),
                Node::Group(g) => if let Some(p) = find(&g.children) { return Some(p); },
                Node::Clip(_) => {}
            }
        }
        None
    }
    find(&doc.nodes).expect("path")
}

#[test]
fn literal_color_animator_applies() {
    let xml = r##"<animated-vector xmlns:android="http://schemas.android.com/apk/res/android" xmlns:aapt="http://schemas.android.com/aapt">
        <aapt:attr name="android:drawable">
            <vector android:width="24dp" android:height="24dp" android:viewportWidth="24" android:viewportHeight="24">
                <path android:name="p" android:pathData="M0,0H24V24H0Z" android:fillColor="#FF000000" />
            </vector>
        </aapt:attr>
        <target android:name="p"><aapt:attr name="android:animation">
            <objectAnimator android:propertyName="fillColor" android:valueType="colorType"
                android:valueFrom="#FF000000" android:valueTo="#FFFFFFFF" android:duration="1000" />
        </aapt:attr></target>
    </animated-vector>"##;
    let result = parse(xml);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let doc = result.doc.unwrap();
    let mid = apply_animations(&doc, 500);
    let Some(Paint::Color(ColorSpec::Literal(c))) = &first_path(&mid).fill else { panic!("fill") };
    assert!((c.r as i32 - 128).abs() <= 1, "r = {}", c.r);
}

#[test]
fn start_offset_does_not_clobber_static_value() {
    let xml = r##"<animated-vector xmlns:android="http://schemas.android.com/apk/res/android" xmlns:aapt="http://schemas.android.com/aapt">
        <aapt:attr name="android:drawable">
            <vector android:width="24dp" android:height="24dp" android:viewportWidth="24" android:viewportHeight="24">
                <group android:name="g" android:rotation="45" />
            </vector>
        </aapt:attr>
        <target android:name="g"><aapt:attr name="android:animation">
            <objectAnimator android:propertyName="rotation" android:valueFrom="0" android:valueTo="360"
                android:startOffset="500" android:duration="1000" />
        </aapt:attr></target>
    </animated-vector>"##;
    let doc = parse(xml).doc.unwrap();
    assert!(parse(xml).diagnostics.is_empty());
    let early = apply_animations(&doc, 0);
    let Node::Group(g) = &early.nodes[0] else { panic!("group") };
    assert!((g.rotation - 45.0).abs() < 1e-6, "rotation = {}", g.rotation);
}

#[test]
fn set_repeat_and_value_to_only_parse() {
    let xml = r##"<animated-vector xmlns:android="http://schemas.android.com/apk/res/android" xmlns:aapt="http://schemas.android.com/aapt">
        <aapt:attr name="android:drawable">
            <vector android:width="24dp" android:height="24dp" android:viewportWidth="24" android:viewportHeight="24">
                <group android:name="g" />
            </vector>
        </aapt:attr>
        <target android:name="g"><aapt:attr name="android:animation">
            <set android:repeatCount="infinite">
                <objectAnimator android:propertyName="rotation" android:valueTo="360" android:duration="1000" />
            </set>
        </aapt:attr></target>
    </animated-vector>"##;
    let result = parse(xml);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let doc = result.doc.unwrap();
    assert_eq!(doc.animations[0].animators[0].repeat_count, -1);
}

#[test]
fn root_vector_alpha_is_animatable() {
    let xml = r##"<animated-vector xmlns:android="http://schemas.android.com/apk/res/android" xmlns:aapt="http://schemas.android.com/aapt">
        <aapt:attr name="android:drawable">
            <vector android:name="root" android:width="24dp" android:height="24dp"
                android:viewportWidth="24" android:viewportHeight="24" android:alpha="1" />
        </aapt:attr>
        <target android:name="root"><aapt:attr name="android:animation">
            <objectAnimator android:propertyName="alpha" android:valueFrom="1" android:valueTo="0"
                android:duration="1000" />
        </aapt:attr></target>
    </animated-vector>"##;
    let result = parse(xml);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let doc = result.doc.unwrap();
    assert_eq!(doc.name.as_deref(), Some("root"));
    let mid = apply_animations(&doc, 500);
    assert!((mid.alpha - 0.5).abs() < 1e-6, "alpha = {}", mid.alpha);
}

#[test]
fn invalid_path_animator_value_warns() {
    let xml = r##"<animated-vector xmlns:android="http://schemas.android.com/apk/res/android" xmlns:aapt="http://schemas.android.com/aapt">
        <aapt:attr name="android:drawable">
            <vector android:width="24dp" android:height="24dp" android:viewportWidth="24" android:viewportHeight="24">
                <path android:name="p" android:pathData="M0,0H10V10H0Z" />
            </vector>
        </aapt:attr>
        <target android:name="p"><aapt:attr name="android:animation">
            <objectAnimator android:propertyName="pathData" android:valueType="pathType"
                android:valueFrom="M0 0 ?!" android:valueTo="M1 1 Z" android:duration="1000" />
        </aapt:attr></target>
    </animated-vector>"##;
    let result = parse(xml);
    assert!(result.diagnostics.iter().any(|d| d.message.contains("invalid pathType")), "{:?}", result.diagnostics);
}

#[test]
fn multiple_roots_are_reported() {
    let xml = r##"<vector xmlns:android="http://schemas.android.com/apk/res/android" android:width="24dp" android:height="24dp" android:viewportWidth="24" android:viewportHeight="24" />
<vector xmlns:android="http://schemas.android.com/apk/res/android" android:width="24dp" android:height="24dp" android:viewportWidth="24" android:viewportHeight="24" />"##;
    let result = parse(xml);
    assert!(result.diagnostics.iter().any(|d| d.message.contains("multiple root")), "{:?}", result.diagnostics);
}

#[test]
fn t_after_c_uses_current_point() {
    let path = vector_model::path::parse_path_data("M0 0 C0 10 10 10 10 0 T20 0").unwrap();
    let elements: Vec<_> = path.elements().iter().collect();
    match elements[2] {
        kurbo::PathEl::QuadTo(c, _) => {
            assert!((c.x - 10.0).abs() < 1e-6 && (c.y - 0.0).abs() < 1e-6, "control = {:?}", c);
        }
        other => panic!("unexpected element {:?}", other),
    }
}
