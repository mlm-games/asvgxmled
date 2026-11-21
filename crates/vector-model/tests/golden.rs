use vector_model::{Rgba, format, parse};

const ICON: &str = r##"<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="48dp" android:height="48dp"
    android:viewportWidth="24" android:viewportHeight="24"
    android:tint="?attr/colorControlNormal">
    <group android:rotation="12" android:pivotX="12" android:pivotY="12">
        <clip-path android:pathData="M0,0H24V24H0Z" />
        <path android:pathData="M12,2L21,7V17L12,22L3,17V7Z"
            android:fillColor="#FF6750A4" android:fillType="evenOdd" />
        <path android:pathData="M4,20 C4,16 8,12 12,12 S20,16 20,20"
            android:strokeColor="#FFFFFFFF" android:strokeWidth="2"
            android:strokeLineCap="round" />
    </group>
</vector>
"##;

#[test]
fn parses_android_icon_and_exports_svg() {
    let result = parse(ICON);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let doc = result.doc.expect("parses");

    assert_eq!((doc.width, doc.height), (48.0, 48.0));
    assert_eq!((doc.viewport_width, doc.viewport_height), (24.0, 24.0));
    assert!(doc.tint.is_some());
    assert_eq!(doc.nodes.len(), 1);

    let svg = format::to_svg(&doc, Rgba::BLACK);
    assert!(svg.contains("fill-rule=\"evenodd\""));
    assert!(svg.contains("stroke-linecap=\"round\""));
    assert!(svg.contains("viewBox=\"0 0 24 24\""));
    assert!(svg.contains("rotate(12"));
}

const ANIMATED: &str = r##"<animated-vector xmlns:android="http://schemas.android.com/apk/res/android"
    xmlns:aapt="http://schemas.android.com/aapt">
    <aapt:attr name="android:drawable">
        <vector android:width="24dp" android:height="24dp"
            android:viewportWidth="24" android:viewportHeight="24">
            <group android:name="spin" android:pivotX="12" android:pivotY="12">
                <path android:name="shape" android:pathData="M4,4H20V20H4Z" android:fillColor="#FF000000" />
            </group>
        </vector>
    </aapt:attr>
    <target android:name="spin">
        <aapt:attr name="android:animation">
            <objectAnimator android:propertyName="rotation" android:valueFrom="0"
                android:valueTo="360" android:duration="2000" android:repeatCount="-1" />
        </aapt:attr>
    </target>
</animated-vector>
"##;

#[test]
fn parses_and_applies_animated_vector() {
    let result = parse(ANIMATED);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let doc = result.doc.expect("animated parses");
    assert_eq!(vector_model::total_duration(&doc), 2000);
    assert_eq!(doc.animations.len(), 1);

    let halfway = vector_model::apply_animations(&doc, 1000);
    if let vector_model::Node::Group(g) = &halfway.nodes[0] {
        assert!((g.rotation - 180.0).abs() < 1.0, "rotation = {}", g.rotation);
    } else {
        panic!("expected group");
    }
}

#[test]
fn optimize_keeps_rendering_semantics() {
    let (optimized, _, _) = format::optimize(ICON);
    let before = parse(ICON).doc.expect("before parses");
    let after = parse(&optimized).doc.expect("after parses");
    assert_eq!(before.width, after.width);
    assert_eq!(before.viewport_width, after.viewport_width);
    assert_eq!(before.nodes.len(), after.nodes.len());
    assert!(optimized.len() < ICON.len());
    assert!(optimized.contains("xmlns:android"));
    let pretty = format::prettify(ICON);
    assert!(pretty.contains("xmlns:android"));
    assert!(pretty.contains("</vector>"));
}
