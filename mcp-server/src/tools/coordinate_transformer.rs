pub struct CoordinateTransformer;

impl CoordinateTransformer {
    pub fn cell_to_css(cell: &str, vp_width: f32, vp_height: f32, offset_u: f32, offset_v: f32) -> Result<(f32, f32), &'static str> {
        if cell.len() < 2 {
            return Err("Invalid cell ID");
        }
        
        let col_char = cell.chars().next().unwrap().to_ascii_uppercase();
        let col_idx = (col_char as u8).saturating_sub(b'A') as f32;
        
        let row_str = &cell[1..];
        let row_idx: f32 = row_str.parse::<f32>().map_err(|_| "Invalid row number")? - 1.0;
        
        let cols = 16.0;
        let rows = (16.0 * (vp_height / vp_width)).round();
        
        let cell_w = vp_width / cols;
        let cell_h = vp_height / rows;
        
        let css_x = (col_idx + 0.5 + offset_u) * cell_w;
        let css_y = (row_idx + 0.5 + offset_v) * cell_h;
        
        Ok((css_x, css_y))
    }
}
