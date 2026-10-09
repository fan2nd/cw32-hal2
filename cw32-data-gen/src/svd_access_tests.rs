//! Synthetic parser/projection tests, not claims about hardware.
use super::*;
use chiptool::svd2ir::NamespaceMode;

fn xml(default: &str, peripherals: &str) -> String {
    format!(
        r#"<device schemaVersion="1.3"><name>TEST</name><version>1</version><description>Parser fixture</description><addressUnitBits>8</addressUnitBits><width>32</width><size>32</size>{default}<peripherals>{peripherals}</peripherals></device>"#
    )
}
fn peripheral(body: &str) -> String {
    format!(
        "<peripheral><name>P</name><headerStructName>P</headerStructName><baseAddress>0</baseAddress>{body}</peripheral>"
    )
}
fn field(name: &str, offset: u32, access: &str) -> String {
    format!(
        "<field><name>{name}</name><bitOffset>{offset}</bitOffset><bitWidth>1</bitWidth>{access}</field>"
    )
}
fn register(name: &str, offset: u32, before: &str, fields: &str) -> String {
    format!(
        "<register><name>{name}</name><addressOffset>{offset}</addressOffset>{before}<fields>{fields}</fields></register>"
    )
}
fn projected(xml: &str) -> Capture {
    let mut c = Capture::parse(xml).unwrap();
    for p in c.device.peripherals.clone() {
        let ir = chiptool::commands::extract_peripheral(&p, NamespaceMode::None).unwrap();
        c.project(&p.name, &ir).unwrap();
    }
    c
}
#[test]
fn access_precedence_tracks_the_actual_declaring_level() {
    let fields = format!(
        "{}{}",
        field("A", 0, ""),
        field("B", 1, "<access>write-only</access>")
    );
    let a = register("A", 0, "", &fields);
    let b = register(
        "B",
        4,
        "<access>read-writeOnce</access>",
        &field("X", 0, ""),
    );
    let cluster = format!(
        "<cluster><name>C</name><addressOffset>16</addressOffset><access>read-only</access>{a}{b}</cluster>"
    );
    let p = peripheral(&format!(
        "<access>writeOnce</access><registers>{cluster}{}</registers>",
        register("D", 64, "", &field("Y", 0, ""))
    ));
    let c = projected(&xml("<access>read-write</access>", &p));
    let result: Vec<_> = c
        .fields
        .iter()
        .map(|f| {
            (
                f.effective_access.as_deref(),
                f.origin.as_ref().unwrap().level.as_str(),
                f.origin.as_ref().unwrap().path.as_str(),
            )
        })
        .collect();
    assert_eq!(
        result,
        vec![
            (Some("read-only"), "cluster", "P.C"),
            (Some("write-only"), "field", "P.C.A.B"),
            (Some("read-writeOnce"), "register", "P.C.B"),
            (Some("writeOnce"), "peripheral", "P")
        ]
    );
}
#[test]
fn device_default_and_unspecified_are_distinct() {
    let p = peripheral(&format!(
        "<registers>{}</registers>",
        register("R", 0, "", &field("F", 0, ""))
    ));
    let c = projected(&xml("<access>read-only</access>", &p));
    assert_eq!(c.fields[0].origin.as_ref().unwrap().level, "device");
    let c = projected(&xml("", &p));
    assert!(c.fields[0].effective_access.is_none());
    assert!(c.fields[0].origin.is_none());
}
#[test]
fn derived_values_resolve_before_destination_defaults() {
    let base = peripheral(&format!(
        "<access>read-only</access><registers>{}</registers>",
        register(
            "R",
            0,
            "",
            &format!(
                "{}<field derivedFrom=\"A\"><name>B</name><bitOffset>1</bitOffset><bitWidth>1</bitWidth><access>writeOnce</access></field>",
                field("A", 0, "")
            )
        )
    ));
    let derived = "<peripheral derivedFrom=\"P\"><name>Q</name><baseAddress>256</baseAddress><access>write-only</access></peripheral>";
    let c = projected(&xml(
        "<access>read-write</access>",
        &format!("{base}{derived}"),
    ));
    assert_eq!(c.fields[0].effective_access.as_deref(), Some("read-only"));
    assert_eq!(c.fields[2].effective_access.as_deref(), Some("write-only"));
    assert_eq!(c.fields[2].origin.as_ref().unwrap().path, "Q");
    assert_eq!(c.fields[3].effective_access.as_deref(), Some("writeOnce"));
    assert_eq!(c.fields[3].origin.as_ref().unwrap().path, "P.R.B");
}
#[test]
fn derived_register_and_cluster_keep_explicit_overrides() {
    let r = register("R", 0, "<access>read-only</access>", &field("F", 0, ""));
    let derived = "<register derivedFrom=\"R\"><name>S</name><addressOffset>4</addressOffset><access>read-writeOnce</access></register>";
    let cluster = format!(
        "<cluster><name>C</name><addressOffset>0</addressOffset>{r}{derived}</cluster><cluster derivedFrom=\"C\"><name>D</name><addressOffset>16</addressOffset><access>write-only</access></cluster>"
    );
    let c = projected(&xml(
        "",
        &peripheral(&format!("<registers>{cluster}</registers>")),
    ));
    assert_eq!(c.fields.len(), 4);
    assert_eq!(c.fields[0].effective_access.as_deref(), Some("read-only"));
    assert_eq!(
        c.fields[1].effective_access.as_deref(),
        Some("read-writeOnce")
    );
    assert_eq!(c.fields[2].effective_access.as_deref(), Some("read-only"));
    assert_eq!(
        c.fields[3].effective_access.as_deref(),
        Some("read-writeOnce")
    );
}
#[test]
fn all_access_modes_and_independent_command_semantics_survive() {
    for access in [
        "read-only",
        "write-only",
        "read-write",
        "writeOnce",
        "read-writeOnce",
    ] {
        let commands = format!(
            "<access>{access}</access><modifiedWriteValues>oneToClear</modifiedWriteValues><readAction>clear</readAction><writeConstraint><range><minimum>0</minimum><maximum>1</maximum></range></writeConstraint>"
        );
        let p = peripheral(&format!(
            "<registers>{}</registers>",
            register(
                "R",
                0,
                "<modifiedWriteValues>zeroToClear</modifiedWriteValues>",
                &field("F", 0, &commands)
            )
        ));
        let c = projected(&xml("", &p));
        let f = &c.fields[0];
        assert_eq!(f.raw_access.as_deref(), Some(access));
        assert_eq!(f.effective_access.as_deref(), Some(access));
        assert_eq!(
            f.field_semantics.modified_write_values.as_deref(),
            Some("oneToClear")
        );
        assert_eq!(f.field_semantics.read_action.as_deref(), Some("clear"));
        assert_eq!(
            f.field_semantics.write_constraint,
            Some(json!({"range":{"minimum":0,"maximum":1}}))
        );
        assert_eq!(
            f.register_semantics.modified_write_values.as_deref(),
            Some("zeroToClear")
        );
    }
}
#[test]
fn dimensions_and_non_numeric_indices_have_checked_projection() {
    let f = "<field><dim>2</dim><dimIncrement>1</dimIncrement><dimIndex>X,Y</dimIndex><name>F[%s]</name><bitOffset>0</bitOffset><bitWidth>1</bitWidth><access>read-only</access></field>";
    let r = register(
        "R[%s]",
        0,
        "<dim>2</dim><dimIncrement>4</dimIncrement><dimIndex>A,B</dimIndex>",
        f,
    );
    let p = peripheral(&format!("<registers>{r}</registers>"));
    let mut c = projected(&xml("", &p));
    assert_eq!(c.fields[0].dimensions.len(), 2);
    assert_eq!(c.fields[0].dimensions[0].indices, vec!["A", "B"]);
    assert_eq!(c.fields[0].dimensions[1].indices, vec!["X", "Y"]);
    let mut ir =
        chiptool::commands::extract_peripheral(&c.device.peripherals[0], NamespaceMode::None)
            .unwrap();
    ir.fieldsets.get_mut("R").unwrap().fields[0].array = None;
    assert!(
        c.project("P", &ir)
            .unwrap_err()
            .to_string()
            .contains("array projection changed")
    );
}
#[test]
fn cycles_unresolved_and_unsupported_references_fail_with_location() {
    let cases = [
        (
            "<field derivedFrom=\"B\"><name>A</name><bitOffset>0</bitOffset><bitWidth>1</bitWidth></field><field derivedFrom=\"A\"><name>B</name><bitOffset>1</bitOffset><bitWidth>1</bitWidth></field>",
            "cyclic SVD derivedFrom at P.R",
        ),
        (
            "<field derivedFrom=\"MISSING\"><name>A</name><bitOffset>0</bitOffset><bitWidth>1</bitWidth></field>",
            "P.R.A -> P.R.MISSING",
        ),
        (
            "<field><dim>2</dim><dimIncrement>1</dimIncrement><name>F%s</name><bitOffset>0</bitOffset><bitWidth>1</bitWidth></field><field derivedFrom=\"F0\"><name>A</name><bitOffset>2</bitOffset><bitWidth>1</bitWidth></field>",
            "P.R.A -> P.R.F0",
        ),
    ];
    for (fields, error) in cases {
        let source = xml(
            "",
            &peripheral(&format!(
                "<registers>{}</registers>",
                register("R", 0, "", fields)
            )),
        );
        assert!(
            Capture::parse(&source)
                .err()
                .unwrap()
                .to_string()
                .contains(error)
        );
    }
}
#[test]
fn duplicate_source_or_normalized_names_are_rejected() {
    let fields = format!("{}{}", field("F", 0, ""), field("F", 1, ""));
    assert!(
        Capture::parse(&xml(
            "",
            &peripheral(&format!(
                "<registers>{}</registers>",
                register("R", 0, "", &fields)
            ))
        ))
        .err()
        .unwrap()
        .to_string()
        .contains("duplicate SVD declaration P.R.F")
    );
    let fields = format!("{}{}", field("F", 0, ""), field("F%s", 1, ""));
    let mut c = Capture::parse(&xml(
        "",
        &peripheral(&format!(
            "<registers>{}</registers>",
            register("R", 0, "", &fields)
        )),
    ))
    .unwrap();
    let ir = chiptool::commands::extract_peripheral(&c.device.peripherals[0], NamespaceMode::None)
        .unwrap();
    assert!(
        c.project("P", &ir)
            .unwrap_err()
            .to_string()
            .contains("ambiguous/lost SVD field")
    );
}

#[test]
fn instance_descriptions_do_not_change_the_shared_block_owner() {
    let base = peripheral(&format!(
        "<description>Shared timer</description><registers>{}</registers>",
        register("R", 0, "", &field("F", 0, "<access>read-only</access>"))
    ));
    let alias = "<peripheral derivedFrom=\"P\"><name>Q</name><description>Second timer instance</description><baseAddress>256</baseAddress></peripheral>";
    let c = projected(&xml(
        "<access>read-write</access>",
        &format!("{base}{alias}"),
    ));
    let a = chiptool::commands::extract_peripheral(&c.device.peripherals[0], NamespaceMode::None)
        .unwrap();
    let b = chiptool::commands::extract_peripheral(&c.device.peripherals[1], NamespaceMode::None)
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(c.fields[1].origin.as_ref().unwrap().path, "P.R.F");
}

#[test]
fn unsupported_enum_derivation_is_reported_at_the_field() {
    let f = "<field><name>F</name><bitOffset>0</bitOffset><bitWidth>1</bitWidth><enumeratedValues derivedFrom=\"G\"/></field>";
    let source = xml(
        "",
        &peripheral(&format!(
            "<registers>{}</registers>",
            register("R", 0, "", f)
        )),
    );
    let error = Capture::parse(&source).err().unwrap();
    assert!(
        error
            .to_string()
            .contains("unsupported enumeratedValues derivedFrom at P.R.F: G"),
        "{error:#}"
    );
}

#[test]
fn dim_array_index_element_names_are_preserved() {
    let f = "<field><dim>2</dim><dimIncrement>1</dimIncrement><dimArrayIndex><enumeratedValue><name>LEFT</name><value>0</value></enumeratedValue><enumeratedValue><name>RIGHT</name><value>1</value></enumeratedValue></dimArrayIndex><name>F%s</name><bitOffset>0</bitOffset><bitWidth>1</bitWidth></field>";
    let c = projected(&xml(
        "",
        &peripheral(&format!(
            "<registers>{}</registers>",
            register("R", 0, "", f)
        )),
    ));
    assert_eq!(
        c.fields[0].dimensions[0].element_names,
        vec!["LEFT", "RIGHT"]
    );
}

#[test]
fn multihop_derived_nodes_keep_intermediate_child_owners() {
    let r = register("BASE", 0, "", &field("F", 0, "<access>read-only</access>"));
    let middle = format!(
        "<register derivedFrom=\"BASE\"><name>MID</name><addressOffset>4</addressOffset><fields>{}</fields></register>",
        field("G", 0, "<access>write-only</access>")
    );
    let top =
        "<register derivedFrom=\"MID\"><name>TOP</name><addressOffset>8</addressOffset></register>";
    let c = projected(&xml(
        "",
        &peripheral(&format!("<registers>{r}{middle}{top}</registers>")),
    ));
    assert_eq!(c.fields[2].declaration, "P.MID.G");
    assert_eq!(c.fields[2].effective_access.as_deref(), Some("write-only"));
    assert_eq!(c.fields[2].origin.as_ref().unwrap().path, "P.MID.G");

    let r = register("R", 0, "", &field("F", 0, "<access>read-only</access>"));
    let g = register("R", 0, "", &field("G", 0, "<access>write-only</access>"));
    let clusters = format!(
        "<cluster><name>BASE</name><addressOffset>0</addressOffset>{r}</cluster><cluster derivedFrom=\"BASE\"><name>MID</name><addressOffset>16</addressOffset>{g}</cluster><cluster derivedFrom=\"MID\"><name>TOP</name><addressOffset>32</addressOffset></cluster>"
    );
    let c = projected(&xml(
        "",
        &peripheral(&format!("<registers>{clusters}</registers>")),
    ));
    assert_eq!(c.fields[2].declaration, "P.MID.R.G");
    assert_eq!(c.fields[2].effective_access.as_deref(), Some("write-only"));

    let p = peripheral(&format!("<registers>{r}</registers>"));
    let middle = format!(
        "<peripheral derivedFrom=\"P\"><name>Q</name><headerStructName>Q</headerStructName><baseAddress>256</baseAddress><registers>{g}</registers></peripheral>"
    );
    let top = "<peripheral derivedFrom=\"Q\"><name>T</name><headerStructName>T</headerStructName><baseAddress>512</baseAddress></peripheral>";
    let c = projected(&xml("", &format!("{p}{middle}{top}")));
    assert_eq!(c.fields[2].declaration, "Q.R.G");
    assert_eq!(c.fields[2].effective_access.as_deref(), Some("write-only"));
}
