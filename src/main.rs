use std::{ error::Error, ops::Range, thread::sleep, time::Duration };
use window_controller::{WindowController, WindowImage};
use grid_kit::{ Color, Grid, GridMask };



const TOOLTIP_SOURCE_IMAGE:&str = "tooltip.png";
const TOOLTIP_HEADER_HEIGHT_FACTOR:f32 = 0.1;
const TOOLTIP_MOD_RARITY_TABLE_MAX_WIDTH_FACTOR:f32 = 0.2;
const TOOLTIP_HEADER_FILTER:fn(&Color) -> bool = |color| color.0.to_be_bytes()[1..].iter().all(|value| *value > 0xC0);
const TOOLTIP_HEADER_MIN_SIMILARITY:f32 = 0.9;
const RARITY_NODE_MIN_DISTANCE:usize = 5;
const MOD_LEVEL_COLOR:[u32; 5] = [0xFF939393, 0xFF178764, 0xFF397CC8, 0xFF724EA4, 0xFFD8A449];
const SCREEN_READ_INTERVAL:Duration = Duration::from_millis(1000 / 10);



fn main() -> Result<(), Box<dyn Error>> {

	// Set always-on top.
	if let Some(console_window) = WindowController::find_one(|window| window.process_name().map(|process_name| process_name == "once_human_mod_level_detector.exe").unwrap_or(false)) {
		console_window.style().set_always_on_top(true).apply();
	}

	// Create tooltip header stamp.
	let tooltip:Grid<Color> = Grid::from_png(TOOLTIP_SOURCE_IMAGE)?;
	let tooltip_header:Grid<bool> = tooltip.sub_grid([0, 0, tooltip.width(), (tooltip.height() as f32 * TOOLTIP_HEADER_HEIGHT_FACTOR) as usize]).map(TOOLTIP_HEADER_FILTER);
	let tooltip_header:GridMask = GridMask::new(tooltip_header);
	let tooltip_mod_rarity_max_width:usize = (tooltip_header.width() as f32 * TOOLTIP_MOD_RARITY_TABLE_MAX_WIDTH_FACTOR) as usize;
	let tooltip_mod_rarity_max_height:usize = tooltip.height() - tooltip_header.height();

	// Keep reading the screen, searching for possible rarity readings.
	loop {
		let active_window:WindowController = WindowController::active();
		if active_window.process_name().map(|process_name| process_name == "ONCE_HUMAN.exe").unwrap_or(false) {
			match get_screen_rarity(&active_window, &tooltip_header, tooltip_mod_rarity_max_width, tooltip_mod_rarity_max_height) {
				Ok(potential_rarity) => match potential_rarity {
					Some(rarity) => println!("{}\t({})", rarity.iter().cloned().sum::<usize>(), rarity.iter().map(|value| value.to_string()).collect::<Vec<String>>().join(" + ")),
					None => {}
				},
				Err(error) => eprintln!("{error}")
			}
		}
		sleep(SCREEN_READ_INTERVAL);
	}
}


fn get_screen_rarity(window:&WindowController, tooltip_header_mask:&GridMask, tooltip_rarity_max_width:usize, tooltip_rarity_max_height:usize) -> Result<Option<Vec<usize>>, Box<dyn Error>> {

	// Take screenshot.
	let screen:WindowImage = window.create_window_image()?;
	let screen:Grid<Color> = Grid::new(screen.data, screen.width, screen.height).map(|color| Color::new(color));

	// Find tooltip location.
	match screen.map_ref(TOOLTIP_HEADER_FILTER).find_masked(tooltip_header_mask.grid(), tooltip_header_mask, TOOLTIP_HEADER_MIN_SIMILARITY) {
		Some(tooltip_header_position) => {

			// Find the rarity nodes.
			let rarity_table:Grid<&Color> = screen.sub_grid([tooltip_header_position[0], tooltip_header_position[1], tooltip_rarity_max_width, tooltip_rarity_max_height]);
			if rarity_table.width() != tooltip_rarity_max_width || rarity_table.height() != tooltip_rarity_max_height {
				return Ok(None);
			}
			let rarity_nodes_offsets:Vec<(usize, usize)> = mod_rarity_offsets(&rarity_table.map(|value| MOD_LEVEL_COLOR.contains(&value.0)));

			// Get the color of each node and return the rarity.
			let mut rarity:Vec<usize> = Vec::new();
			for node_offset in rarity_nodes_offsets {
				let rarity_color:u32 = screen[(tooltip_header_position[0] + node_offset.0, tooltip_header_position[1] + node_offset.1)].0;
				if let Some(rarity_level_index) = MOD_LEVEL_COLOR.iter().position(|color| *color == rarity_color) {
					rarity.push(rarity_level_index + 1);
				} else {
					return Ok(None);
				}
			}
			Ok(Some(rarity))
		},
		None => Ok(None)
	}
}


fn mod_rarity_offsets(rarity_map:&Grid<bool>) -> Vec<(usize, usize)> {

	// Find potential locations.
	let mut rarity_node_x_ranges:Vec<(Range<usize>, usize)> = Vec::new();
	let mut previous_x_width:usize = 0;
	for y in 0..rarity_map.height() - 1 {

		// Find largest positive range in this row.
		let mut largest_x_range:Range<usize> = 0..0;
		for x_start in 0..rarity_map.width() {
			if rarity_map[(x_start, y)] {
				let x_end:usize = x_start + (x_start..rarity_map.width()).take_while(|end| rarity_map[(*end, y)]).count();
				if x_end - x_start > largest_x_range.len() {
					largest_x_range = x_start..x_end;
				}
			}
		}

		// If this range is wider then the previous and next row, this is a peak, append it to the ranges list.
		let x_width:usize = largest_x_range.len();
		if x_width > 0 && previous_x_width <= x_width && largest_x_range.clone().filter(|x| rarity_map[(*x, y + 1)]).count() < x_width {
			rarity_node_x_ranges.push((largest_x_range, y));
		}
		previous_x_width = x_width;
	}

	// Keep best locations.
	rarity_node_x_ranges.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
	if rarity_node_x_ranges.len() > 4 {
		rarity_node_x_ranges.drain(4..);
	}
	let mut index:usize = 1;
	while index < rarity_node_x_ranges.len() {
		if rarity_node_x_ranges[index].1 - rarity_node_x_ranges[index - 1].1 < RARITY_NODE_MIN_DISTANCE {
			rarity_node_x_ranges.remove(index);
		} else {
			index += 1;
		}
	}
	let rarity_nodes_offsets:Vec<(usize, usize)> = rarity_node_x_ranges.into_iter().map(|(x_range, y)| (x_range.start + x_range.len() / 2, y)).collect();

	// Return found locations.
	rarity_nodes_offsets
}