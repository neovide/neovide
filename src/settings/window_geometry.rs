#[derive(Debug, Clone)]
pub struct WindowGeometry {
    pub width: u64,
    pub height: u64,
}

pub const DEFAULT_WINDOW_GEOMETRY: WindowGeometry = WindowGeometry {
    width: 100,
    height: 50,
};

pub fn parse_window_geometry(geometry: Option<String>) -> Result<WindowGeometry, String> {
    geometry.map_or(Ok(DEFAULT_WINDOW_GEOMETRY), |input| {
        let invalid_parse_err = format!(
            "Invalid geometry: {}\nValid format: <width>x<height>",
            input
        );

        input
            .split('x')
            .map(|dimension| {
                dimension
                    .parse::<u64>()
                    .map_err(|_| invalid_parse_err.as_str())
                    .and_then(|dimension| {
                        if dimension > 0 {
                            Ok(dimension)
                        } else {
                            Err("Invalid geometry: Window dimensions should be greater than 0.")
                        }
                    })
            })
            .collect::<Result<Vec<_>, &str>>()
            .and_then(|dimensions| {
                if let [width, height] = dimensions[..] {
                    Ok(WindowGeometry { width, height })
                } else {
                    Err(invalid_parse_err.as_str())
                }
            })
            .map_err(|msg| msg.to_owned())
    })
}
