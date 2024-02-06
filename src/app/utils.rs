use eframe::egui;

pub fn is_dragvalue_finished(res: egui::Response) -> bool {
    !res.has_focus() && (res.drag_released() || res.lost_focus())
}
