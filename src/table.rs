use comfy_table::presets::NOTHING;
use comfy_table::{Cell, Color, ContentArrangement, Table};

pub fn make_table(headers: Vec<&str>) -> Table {
    let mut table = Table::new();
    table
        .load_preset(NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic);

    let header_cells: Vec<Cell> = headers
        .into_iter()
        .map(|h| {
            Cell::new(h)
                .fg(Color::Cyan)
                .add_attribute(comfy_table::Attribute::Bold)
        })
        .collect();
    table.set_header(header_cells);

    table
}
