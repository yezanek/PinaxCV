//! Every user-facing UI string (spec section 2, ADR-050). English is the only
//! locale in v1; another locale is one more `Strings` value. CV content,
//! headings and dates go through `cv-core::locale` instead.
//!
//! Strings with values in them are templates: `{name}` placeholders are
//! filled by [`fill`], so a translation can move them around.

use cv_api::{ApiError, ErrorCode};

/// The UI text for one locale.
pub struct Strings {
    pub app_name: &'static str,

    // CV list
    pub your_cvs: &'static str,
    pub delete: &'static str,
    pub new_cv: &'static str,
    pub new_cv_title_placeholder: &'static str,
    pub new_cv_title: &'static str,
    pub no_cvs_yet: &'static str,
    pub duplicate: &'static str,
    /// `{title}`: the title of a duplicated CV.
    pub copy_title: &'static str,
    /// `{title}`.
    pub deleted_cv: &'static str,
    /// `{date}`.
    pub edited: &'static str,
    /// `{day}`, `{month}` (from `months`), `{year}` and `{time}` (HH:MM).
    pub date_time: &'static str,
    pub months: [&'static str; 12],
    pub one_page: &'static str,
    /// `{n}`.
    pub n_pages: &'static str,
    pub one_variant: &'static str,
    /// `{n}`.
    pub n_variants: &'static str,
    pub import_cv: &'static str,
    pub import_cv_hint: &'static str,

    // Import wizard
    pub import_title: &'static str,
    pub import_no_name: &'static str,
    /// `{section}` and `{n}`, the number of items read into it.
    pub import_section_line: &'static str,
    pub import_warnings: &'static str,
    pub import_check_bullets: &'static str,
    pub import_text: &'static str,
    pub import_text_hint: &'static str,
    pub text_import_title: &'static str,
    pub text_import_hint: &'static str,
    pub text_import_paste: &'static str,
    pub text_import_continue: &'static str,
    pub text_import_file: &'static str,
    /// `{host}`: where the text goes.
    pub text_import_sent_to: &'static str,
    pub text_import_sent_label: &'static str,
    /// `{n}`.
    pub text_import_redacted: &'static str,
    pub text_import_nothing_redacted: &'static str,
    pub text_import_needs_ai: &'static str,
    pub text_import_progress: &'static str,
    pub text_import_with_ai: &'static str,
    pub text_import_without_ai: &'static str,
    pub back: &'static str,
    pub import_into: &'static str,
    pub import_as_new: &'static str,
    pub import_merge: &'static str,
    pub import_merge_into: &'static str,
    pub imported_cv_default: &'static str,
    pub import_review_hint: &'static str,
    /// `{value}`.
    pub keep_mine: &'static str,
    /// `{value}`.
    pub use_imported: &'static str,
    pub import_entries: &'static str,
    pub import_add_new: &'static str,
    pub import_skip: &'static str,
    /// `{entry}`: the existing entry the import would overwrite.
    pub import_replace: &'static str,
    pub import_merge_note: &'static str,
    pub import_button: &'static str,

    // Editor bar and status
    pub back_to_cvs: &'static str,
    pub export_pdf: &'static str,
    pub export_html: &'static str,
    pub export_pdf_hint: &'static str,
    pub undo: &'static str,
    pub undo_hint: &'static str,
    pub redo: &'static str,
    pub redo_hint: &'static str,
    pub loading: &'static str,
    pub editing: &'static str,
    pub saving: &'static str,
    pub saved: &'static str,
    pub not_saved: &'static str,
    pub exporting: &'static str,
    pub export_cancelled: &'static str,
    pub export_failed: &'static str,
    /// `{path}`, then `{ats}` (empty, or [`Strings::ats_score`]).
    pub exported_to: &'static str,
    /// `{score}`.
    pub ats_score: &'static str,

    // Editor form and preview
    pub content: &'static str,
    pub preview: &'static str,
    pub updating_preview: &'static str,
    pub zoom: &'static str,
    pub zoom_fit: &'static str,
    pub zoom_actual: &'static str,
    /// `{n}`: how many messages the template compiler gave.
    pub template_messages: &'static str,
    pub severity_info: &'static str,
    pub severity_warning: &'static str,
    pub severity_error: &'static str,
    pub page_preview: &'static str,
    pub cv_title: &'static str,
    pub full_name: &'static str,
    pub headline: &'static str,
    pub email: &'static str,
    pub phone: &'static str,
    pub location: &'static str,
    pub summary: &'static str,

    // Style tab
    pub style: &'static str,
    pub template: &'static str,
    pub paper: &'static str,
    pub paper_a4: &'static str,
    pub paper_letter: &'static str,
    pub cv_language: &'static str,
    pub english: &'static str,
    pub date_style: &'static str,
    pub date_month_name: &'static str,
    pub date_numeric: &'static str,
    pub date_year_only: &'static str,
    pub font_size: &'static str,
    pub line_spacing: &'static str,
    pub margins: &'static str,
    /// Units after a slider's value, with their leading space.
    pub unit_pt: &'static str,
    pub unit_mm: &'static str,
    pub accent: &'static str,
    pub accent_none: &'static str,
    pub accent_reset: &'static str,

    // Section kinds
    pub experience: &'static str,
    pub work_experience: &'static str,
    pub education: &'static str,
    pub projects: &'static str,
    pub certifications: &'static str,
    pub skills: &'static str,
    pub languages: &'static str,

    // Adding and collapsing sections
    pub add_section: &'static str,
    pub add_section_choose: &'static str,
    pub custom_section: &'static str,
    pub custom_title: &'static str,
    pub collapse_section: &'static str,
    pub expand_section: &'static str,
    pub locked: &'static str,
    pub locked_hint: &'static str,
    /// `{n}` characters of at most `{max}`.
    pub summary_count: &'static str,

    // Sections and entries
    pub move_section_up: &'static str,
    pub move_section_down: &'static str,
    pub remove_section: &'static str,
    pub drag_to_reorder: &'static str,
    pub add_entry: &'static str,
    pub remove_entry: &'static str,
    pub move_entry_up: &'static str,
    pub move_entry_down: &'static str,
    pub entry_title: &'static str,
    pub organisation: &'static str,
    pub start: &'static str,
    pub end: &'static str,
    pub date_placeholder: &'static str,
    pub current_job: &'static str,
    pub description: &'static str,
    pub link: &'static str,

    // Bullets
    pub bullet: &'static str,
    pub add_bullet: &'static str,
    pub move_bullet_up: &'static str,
    pub move_bullet_down: &'static str,
    pub remove_bullet: &'static str,

    // Skills and languages
    pub add_skill_group: &'static str,
    pub group_label: &'static str,
    pub skills_line: &'static str,
    pub remove_group: &'static str,
    pub move_group_up: &'static str,
    pub move_group_down: &'static str,
    pub language: &'static str,
    pub level: &'static str,
    pub level_placeholder: &'static str,
    pub add_language: &'static str,
    pub move_language_up: &'static str,
    pub move_language_down: &'static str,
    pub move_skill_up: &'static str,
    pub move_skill_down: &'static str,
    pub edit_bullet_in_variant: &'static str,
    pub edit_description_in_variant: &'static str,
    pub edit_summary_in_variant: &'static str,
    pub edited_in_variant: &'static str,
    pub revert_to_base: &'static str,
    pub page_target: &'static str,
    pub pages_auto: &'static str,
    pub pages_one: &'static str,
    pub pages_two: &'static str,
    pub fit_to_one: &'static str,
    pub fit_to_two: &'static str,
    pub fitting: &'static str,
    pub fit_already: &'static str,
    pub fit_proposed: &'static str,
    pub fit_impossible: &'static str,
    pub proposed_cuts: &'static str,
    pub cut_hide_bullet: &'static str,
    pub cut_collapse_entry: &'static str,
    pub accept: &'static str,
    pub reject: &'static str,
    pub accept_all: &'static str,
    pub checks: &'static str,
    pub lint_heading: &'static str,
    pub lint_saved_only: &'static str,
    pub lint_clean: &'static str,
    pub lint_errors: &'static str,
    pub lint_warnings: &'static str,
    pub lint_tips: &'static str,
    pub show_field: &'static str,
    pub ats_heading: &'static str,
    pub ats_limits: &'static str,
    pub ats_run: &'static str,
    pub ats_running: &'static str,
    pub ats_score_line: &'static str,
    pub check_passed: &'static str,
    pub check_failed: &'static str,
    pub ats_extracted: &'static str,
    pub copy_text: &'static str,
    pub copied: &'static str,
    pub copy_failed: &'static str,
    pub settings: &'static str,
    pub ai_settings: &'static str,
    pub ai_settings_intro: &'static str,
    pub ai_provider: &'static str,
    pub provider_none: &'static str,
    pub provider_anthropic: &'static str,
    pub provider_openai: &'static str,
    pub provider_gemini: &'static str,
    pub provider_compatible: &'static str,
    pub ai_model: &'static str,
    pub ai_model_placeholder: &'static str,
    pub ai_base_url: &'static str,
    pub ai_key: &'static str,
    pub ai_key_optional: &'static str,
    pub key_saved_placeholder: &'static str,
    pub key_in_keychain: &'static str,
    pub key_in_memory: &'static str,
    pub key_none: &'static str,
    pub no_keychain: &'static str,
    pub send_name: &'static str,
    pub send_location: &'static str,
    pub save: &'static str,
    pub settings_saved: &'static str,
    pub test_connection: &'static str,
    pub testing_connection: &'static str,
    pub connection_ok: &'static str,
    pub forget_key: &'static str,
    pub key_forgotten: &'static str,
    pub consent_heading: &'static str,
    pub consent_where: &'static str,
    pub consent_content: &'static str,
    pub consent_job: &'static str,
    pub consent_never: &'static str,
    pub consent_name_on: &'static str,
    pub consent_name_off: &'static str,
    pub consent_location_on: &'static str,
    pub consent_location_off: &'static str,
    pub consent_import: &'static str,
    pub consent_agree: &'static str,
    pub consent_saved: &'static str,
    pub consent_given: &'static str,
    pub defaults_heading: &'static str,
    pub defaults_intro: &'static str,
    pub defaults_saved: &'static str,
    pub data_heading: &'static str,
    pub data_folder_intro: &'static str,
    pub open_data_folder: &'static str,
    pub backup_intro: &'static str,
    pub backup_button: &'static str,
    pub backup_saved: &'static str,
    pub restore_button: &'static str,
    pub restore_warning: &'static str,
    pub restore_confirm: &'static str,
    pub restore_done: &'static str,
    pub clear_cache_intro: &'static str,
    pub clear_cache_button: &'static str,
    pub cache_cleared: &'static str,
    pub delete_all_button: &'static str,
    pub delete_all_warning: &'static str,
    pub delete_all_confirm: &'static str,
    pub all_data_deleted: &'static str,
    pub about_heading: &'static str,
    pub about_version: &'static str,
    pub about_license: &'static str,
    pub about_fonts: &'static str,
    pub about_licences_show: &'static str,
    pub about_licences_hide: &'static str,
    pub about_licences_label: &'static str,
    pub about_privacy: &'static str,
    pub remove_language: &'static str,

    // Variants
    pub version: &'static str,
    pub base_cv: &'static str,
    pub new_variant: &'static str,
    /// `{n}`: the default name of a new variant.
    pub variant_default_name: &'static str,
    pub duplicate_variant: &'static str,
    pub variant_copy_name: &'static str,
    /// `{name}`.
    pub showing_variant: &'static str,
    pub variant_name: &'static str,
    pub delete_variant: &'static str,
    pub delete_variant_confirm: &'static str,
    pub cancel: &'static str,
    pub shown_in_variant: &'static str,
    pub one_line: &'static str,
    pub untitled_entry: &'static str,
    pub empty_bullet: &'static str,
    pub unlabelled_group: &'static str,

    // Tailoring (M7a)
    pub tailor_heading: &'static str,
    pub job_title: &'static str,
    pub company: &'static str,
    pub job_description: &'static str,
    pub reorder_by_relevance: &'static str,
    pub analyze_and_tailor: &'static str,
    pub tailor_needs_ai: &'static str,
    pub tailor_progress: &'static str,
    pub stage_starting: &'static str,
    pub stage_job_parse: &'static str,
    pub stage_relevance: &'static str,
    pub stage_selection: &'static str,
    pub stage_fit: &'static str,
    pub stage_rewrite: &'static str,
    pub stage_summary: &'static str,
    pub stage_gaps: &'static str,
    pub stage_review: &'static str,
    pub stage_working: &'static str,
    /// `{n}`.
    pub tailor_done: &'static str,
    pub tailor_nothing: &'static str,
    pub tailor_cancelled: &'static str,
    pub rewrite_failed: &'static str,
    pub summary_failed: &'static str,
    pub gaps_failed: &'static str,
    /// `{n}`.
    pub discarded_count: &'static str,
    pub discarded_before: &'static str,
    pub discarded_after: &'static str,

    // Suggestion review (M7a)
    pub review_heading: &'static str,
    pub review_hint: &'static str,
    pub accept_all_safe: &'static str,
    pub reject_all: &'static str,
    pub accept_all_confirm: &'static str,
    pub group_selection: &'static str,
    pub group_order: &'static str,
    pub group_rewrites: &'static str,
    pub group_summary: &'static str,
    pub group_skills_order: &'static str,
    pub removed_item: &'static str,
    pub rewrite_summary: &'static str,
    pub reorder_bullets_of: &'static str,
    pub reorder_entries_of: &'static str,
    pub reorder_skills_of: &'static str,
    pub reorder_items: &'static str,
    pub suggest_hide: &'static str,
    pub suggest_collapse: &'static str,
    pub suggest_show: &'static str,
    pub suggest_rewrite: &'static str,
    /// `{item}`: the entry the bullet belongs to.
    pub rewrite_bullet_in: &'static str,
    pub edit_then_accept: &'static str,
    pub edited_text: &'static str,
    pub needs_attention: &'static str,
    pub badge_new_number: &'static str,
    pub badge_new_term: &'static str,
    pub badge_terminology: &'static str,
    pub badge_length: &'static str,
    pub badge_language: &'static str,

    // Gap questions (spec section 12.6)
    pub gaps_heading: &'static str,
    pub gaps_hint: &'static str,
    pub gap_missing: &'static str,
    pub gap_weak: &'static str,
    pub gap_must_have: &'static str,
    pub gap_nice_to_have: &'static str,
    /// `{hint}`: where the model suggests adding it.
    pub gap_where: &'static str,
    pub add_to_cv: &'static str,
    pub dismiss: &'static str,

    // AI review of the base CV (spec section 11.11)
    pub ai_review_heading: &'static str,
    pub ai_review_hint: &'static str,
    pub review_cv_with_ai: &'static str,
    pub ai_review_progress: &'static str,
    /// `{n}`.
    pub ai_review_done: &'static str,
    pub ai_review_nothing: &'static str,
    pub ai_review_cancelled: &'static str,
    /// `{at}`: when the shown review ran.
    pub ai_review_last: &'static str,
    pub ai_review_notes: &'static str,
    pub note_impact: &'static str,
    pub note_clarity: &'static str,
    pub note_structure: &'static str,
    pub note_consistency: &'static str,
    pub whole_cv: &'static str,
    pub wait_for_save: &'static str,

    // Errors raised by the UI itself
    pub no_secure_random: &'static str,
    pub app_not_answering: &'static str,
    /// `{error}`.
    pub unexpected_data: &'static str,

    // Errors from the backend, by code (see [`error_message`])
    pub error_not_found: &'static str,
    pub error_conflict: &'static str,
    pub error_cancelled: &'static str,
    pub error_keychain: &'static str,
    pub error_ai_not_configured: &'static str,
    pub error_ai_consent_required: &'static str,
    pub error_ai_cancelled: &'static str,
}

/// English (v1's only locale).
pub static EN: Strings = Strings {
    app_name: "PinaxCV",

    your_cvs: "Your CVs",
    delete: "Delete",
    new_cv: "New CV",
    new_cv_title_placeholder: "Title, for example Main CV",
    new_cv_title: "Title of the new CV",
    no_cvs_yet: "No CVs yet. Give your first one a title and choose New CV.",
    duplicate: "Duplicate",
    copy_title: "{title} (copy)",
    deleted_cv: "Deleted \u{201c}{title}\u{201d}.",
    edited: "Edited {date}",
    date_time: "{day} {month} {year}, {time}",
    months: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    one_page: "1 page",
    n_pages: "{n} pages",
    one_variant: "1 variant",
    n_variants: "{n} variants",
    import_cv: "Import…",
    import_cv_hint: "Import a Europass CV (PDF or XML) or another CV saved as PDF",

    import_title: "Import a CV",
    import_no_name: "No name found",
    import_section_line: "{section}: {n}",
    import_warnings: "Check these after importing",
    import_check_bullets: "Not found word for word in your text: check these bullets",
    import_text: "Import text…",
    import_text_hint: "Paste a CV or LinkedIn profile, or read a PDF or text file the other importer cannot",
    text_import_title: "Import text",
    text_import_hint: "Paste the text of a CV or LinkedIn profile, or choose a PDF or text file. You see exactly what would be sent before anything is.",
    text_import_paste: "CV text",
    text_import_continue: "Continue",
    text_import_file: "Choose a PDF or text file…",
    text_import_sent_to: "This is exactly what “Read with AI” sends to {host}:",
    text_import_sent_label: "Text to send",
    text_import_redacted: "Email addresses, phone numbers and links taken out: {n}. They are read from your text on this computer instead.",
    text_import_nothing_redacted: "No email addresses, phone numbers or links were found in the text.",
    text_import_needs_ai: "Set up AI in Settings to read the text with AI, or read it without AI.",
    text_import_progress: "Import progress",
    text_import_with_ai: "Read with AI",
    text_import_without_ai: "Read without AI",
    back: "Back",
    import_into: "Save as",
    import_as_new: "A new CV",
    import_merge: "Merge into an existing CV",
    import_merge_into: "CV to merge into",
    imported_cv_default: "Imported CV",
    import_review_hint: "These details differ. Choose which to keep.",
    keep_mine: "Keep mine: {value}",
    use_imported: "Use imported: {value}",
    import_entries: "Imported entries",
    import_add_new: "Add as new",
    import_skip: "Skip",
    import_replace: "Replace {entry}",
    import_merge_note: "New skills, languages and links are added. A snapshot of the CV is saved first, so the merge can be undone.",
    import_button: "Import",

    back_to_cvs: "← CVs",
    export_pdf: "Export PDF",
    export_html: "Export HTML",
    export_pdf_hint: "Export PDF (Ctrl+E)",
    undo: "Undo",
    undo_hint: "Undo (Ctrl+Z)",
    redo: "Redo",
    redo_hint: "Redo (Ctrl+Shift+Z)",
    loading: "Loading…",
    editing: "Editing…",
    saving: "Saving…",
    saved: "Saved",
    not_saved: "Not saved",
    exporting: "Exporting…",
    export_cancelled: "Export cancelled",
    export_failed: "Export failed",
    exported_to: "Exported to {path}{ats}",
    ats_score: ", ATS score {score}",

    content: "Content",
    preview: "Preview",
    updating_preview: "Updating preview",
    zoom: "Zoom",
    zoom_fit: "Fit width",
    zoom_actual: "100%",
    template_messages: "Template messages: {n}",
    severity_info: "Note:",
    severity_warning: "Warning:",
    severity_error: "Error:",
    page_preview: "CV page preview",
    cv_title: "CV title (only you see this)",
    full_name: "Full name",
    headline: "Headline",
    email: "Email",
    phone: "Phone",
    location: "Location",
    summary: "Summary",

    style: "Style",
    template: "Template",
    paper: "Paper size",
    paper_a4: "A4",
    paper_letter: "US Letter",
    cv_language: "CV language (headings and dates)",
    english: "English",
    date_style: "Date style",
    date_month_name: "Mar 2021",
    date_numeric: "03/2021",
    date_year_only: "2021",
    font_size: "Font size",
    line_spacing: "Line spacing",
    margins: "Margins",
    unit_pt: " pt",
    unit_mm: " mm",
    accent: "Accent colour for headings and rules",
    accent_none: "None (black)",
    accent_reset: "Use black",

    experience: "Experience",
    work_experience: "Work experience",
    education: "Education",
    projects: "Projects",
    certifications: "Certifications",
    skills: "Skills",
    languages: "Languages",

    add_section: "Add section",
    add_section_choose: "Choose a section to add…",
    custom_section: "Custom section",
    custom_title: "Section title",
    collapse_section: "Collapse section",
    expand_section: "Expand section",
    locked: "Locked",
    locked_hint: "Protects this entry's facts (title, organisation, location and dates) from AI suggestions.",
    summary_count: "{n} / {max} characters",

    move_section_up: "Move section up",
    move_section_down: "Move section down",
    remove_section: "Remove section",
    drag_to_reorder: "Drag to reorder",
    add_entry: "+ Entry",
    remove_entry: "Remove entry",
    move_entry_up: "Move entry up",
    move_entry_down: "Move entry down",
    entry_title: "Title",
    organisation: "Organisation",
    start: "Start",
    end: "End",
    date_placeholder: "2021 or 2021-03",
    current_job: "I work here now",
    description: "Description (optional)",
    link: "Link (optional, https:// links are clickable)",

    bullet: "Bullet",
    add_bullet: "+ Bullet",
    move_bullet_up: "Move bullet up",
    move_bullet_down: "Move bullet down",
    remove_bullet: "Remove bullet",

    add_skill_group: "+ Skill group",
    group_label: "Group label (optional)",
    skills_line: "Skills, separated by commas",
    remove_group: "Remove group",
    move_group_up: "Move skill group up",
    move_group_down: "Move skill group down",
    language: "Language",
    level: "Level",
    level_placeholder: "Level, for example C1",
    add_language: "+ Language",
    move_language_up: "Move language up",
    move_language_down: "Move language down",
    move_skill_up: "Move skill up",
    move_skill_down: "Move skill down",
    edit_bullet_in_variant: "Edit text for this variant",
    edit_description_in_variant: "Edit description for this variant",
    edit_summary_in_variant: "Edit summary for this variant",
    edited_in_variant: "edited",
    revert_to_base: "Revert to base",
    page_target: "Length",
    pages_auto: "Auto",
    pages_one: "1 page",
    pages_two: "2 pages",
    fit_to_one: "Fit to 1 page",
    fit_to_two: "Fit to 2 pages",
    fitting: "Finding cuts…",
    fit_already: "It already fits. Nothing to cut.",
    fit_proposed: "Cuts proposed: {n}. Nothing changes until you accept them.",
    fit_impossible: "Even with every cut, this needs {pages} pages.",
    proposed_cuts: "Proposed cuts",
    cut_hide_bullet: "Hide bullet “{text}”",
    cut_collapse_entry: "Show “{entry}” on one line",
    accept: "Accept",
    reject: "Reject",
    accept_all: "Accept all",
    checks: "Checks",
    lint_heading: "Content checks",
    lint_saved_only: "These checks read the saved CV, so they catch up a moment after you type.",
    lint_clean: "No problems found.",
    lint_errors: "Errors",
    lint_warnings: "Warnings",
    lint_tips: "Tips",
    show_field: "Show",
    ats_heading: "ATS check",
    ats_limits: "These are best-practice checks on how parsers read the PDF, not rules published by any applicant tracking system. Still check the final document in the employer's application flow.",
    ats_run: "Run check",
    ats_running: "Checking…",
    ats_score_line: "Score as read by PinaxCV: {score} / 100",
    check_passed: "Passed:",
    check_failed: "Failed:",
    ats_extracted: "Text a parser reads",
    copy_text: "Copy text",
    copied: "Copied.",
    copy_failed: "Could not copy. Select the text and copy it instead.",
    settings: "Settings",
    ai_settings: "AI assistant",
    ai_settings_intro: "AI is optional. It only runs when you ask, and it only suggests changes for you to accept or reject.",
    ai_provider: "Provider",
    provider_none: "Off",
    provider_anthropic: "Anthropic (Claude)",
    provider_openai: "OpenAI",
    provider_gemini: "Google Gemini",
    provider_compatible: "OpenAI-compatible server (for example Ollama)",
    ai_model: "Model",
    ai_model_placeholder: "The model name from your provider",
    ai_base_url: "Server address",
    ai_key: "API key",
    ai_key_optional: "API key (if the server needs one)",
    key_saved_placeholder: "Saved. Type a new key to replace it.",
    key_in_keychain: "The key is stored in your system keychain.",
    key_in_memory: "The key is kept in memory until PinaxCV closes.",
    key_none: "No key is saved.",
    no_keychain: "No system keychain was found, so PinaxCV never writes the key to disk. You will be asked for it again after a restart.",
    send_name: "Send my name",
    send_location: "Send my city and country",
    save: "Save",
    settings_saved: "Saved.",
    test_connection: "Test connection",
    testing_connection: "Testing…",
    connection_ok: "The provider answered.",
    forget_key: "Forget key",
    key_forgotten: "The key was removed.",
    consent_heading: "What PinaxCV sends",
    consent_where: "Requests go to {host}, using your own account there.",
    consent_content: "Your CV content: headline, summary, entries with employer names, bullets and skills.",
    consent_job: "The job description you paste, when you tailor a version.",
    consent_never: "Never your email, phone number or links.",
    consent_name_on: "Your name.",
    consent_name_off: "Not your name.",
    consent_location_on: "Your city and country, nothing more precise.",
    consent_location_off: "Not your location.",
    consent_import: "Importing a CV with AI sends its text without email addresses, phone numbers and links, and shows you that text first.",
    consent_agree: "I agree",
    consent_saved: "Thanks. AI features can be used now.",
    consent_given: "You agreed to what is sent to this provider. Changing the provider asks again.",
    defaults_heading: "New CVs",
    defaults_intro: "What new and imported CVs start with. Each CV keeps its own settings in its Style tab.",
    defaults_saved: "Defaults saved.",
    data_heading: "Your data",
    data_folder_intro: "Your CVs, variants, settings and the log are kept in this folder on this computer only.",
    open_data_folder: "Open folder",
    backup_intro: "A backup holds all your CVs, variants and settings, and saved AI answers. It never holds your API key. It contains personal data: keep it somewhere private.",
    backup_button: "Back up…",
    backup_saved: "Backup saved to {path}.",
    restore_button: "Restore from backup…",
    restore_warning: "Restoring replaces all your CVs, variants and settings with the backup's. A copy of your current data is kept in the data folder first.",
    restore_confirm: "Choose a backup and restore",
    restore_done: "Restored. Your data from before is kept at {path}.",
    clear_cache_intro: "PinaxCV keeps the AI's answers so the same request is not sent twice. They contain text from your CVs and go into backups.",
    clear_cache_button: "Clear AI cache",
    cache_cleared: "Saved AI answers removed.",
    delete_all_button: "Delete all data…",
    delete_all_warning: "This deletes every CV, variant, snapshot, setting and saved AI answer, the copies kept by restores, and your saved API keys. It cannot be undone. Back up first if you might need them.",
    delete_all_confirm: "Delete everything",
    all_data_deleted: "All data was deleted.",
    about_heading: "About PinaxCV",
    about_version: "Version {version}",
    about_license: "PinaxCV is open source under the {license} licence.",
    about_fonts: "CVs use the Inter typeface, under the SIL Open Font License 1.1.",
    about_licences_show: "Show third-party licences",
    about_licences_hide: "Hide third-party licences",
    about_licences_label: "Third-party licences",
    about_privacy: "PinaxCV makes no network requests except to the AI provider you set up and the links you open.",
    remove_language: "Remove language",

    version: "Version",
    base_cv: "Base CV",
    new_variant: "New variant",
    variant_default_name: "Variant {n}",
    duplicate_variant: "Duplicate variant",
    variant_copy_name: "{name} (copy)",
    showing_variant: "Showing variant \u{201c}{name}\u{201d}. The form below edits the base CV; what you change in this panel applies only to this variant.",
    variant_name: "Variant name",
    delete_variant: "Delete variant",
    delete_variant_confirm: "Delete this variant? The base CV is not changed.",
    cancel: "Cancel",
    shown_in_variant: "Shown in this variant",
    one_line: "one line",
    untitled_entry: "(untitled entry)",
    empty_bullet: "(empty bullet)",
    unlabelled_group: "(unlabelled group)",

    tailor_heading: "Tailor to a job",
    job_title: "Job title",
    company: "Company",
    job_description: "Job description (paste the posting)",
    reorder_by_relevance: "Also suggest ordering experience by relevance (recruiters usually expect newest first)",
    analyze_and_tailor: "Analyze and tailor",
    tailor_needs_ai: "Set up an AI provider in Settings to tailor this version automatically. You can still hide, collapse and reorder by hand.",
    tailor_progress: "Tailoring progress",
    stage_starting: "Starting…",
    stage_job_parse: "Reading the job description…",
    stage_relevance: "Scoring your experience against the job…",
    stage_selection: "Choosing what to show…",
    stage_fit: "Fitting to the page target…",
    stage_rewrite: "Suggesting rewrites…",
    stage_summary: "Writing a summary…",
    stage_gaps: "Looking for what the job asks and your CV does not show…",
    stage_review: "Reviewing your CV…",
    stage_working: "Working…",
    tailor_done: "{n} suggestions are ready for review. Nothing changes until you accept them.",
    tailor_nothing: "Tailoring found nothing to change.",
    tailor_cancelled: "Tailoring was cancelled.",
    rewrite_failed: "Rewrites could not be suggested this time; the other suggestions were saved.",
    summary_failed: "A summary could not be suggested this time.",
    gaps_failed: "The job's requirements could not be checked for gaps this time.",
    discarded_count: "{n} suggestions discarded by safety checks",
    discarded_before: "Original",
    discarded_after: "Discarded rewrite",

    review_heading: "Suggestions to review",
    review_hint: "Nothing changes until you accept a suggestion. Rejected ones are kept, so a rerun does not ask again.",
    accept_all_safe: "Accept all without warnings",
    reject_all: "Reject all",
    accept_all_confirm: "Some suggestions have warnings. Accept them all anyway?",
    group_selection: "Hide or shorten",
    group_order: "Order",
    group_rewrites: "Rewrites",
    group_summary: "Summary",
    group_skills_order: "Skills order",
    removed_item: "(no longer in the CV)",
    rewrite_summary: "Rewrite the summary",
    reorder_bullets_of: "Reorder the bullets of {item}",
    reorder_entries_of: "Reorder {item}",
    reorder_skills_of: "Reorder the skills {item}",
    reorder_items: "Reorder",
    suggest_hide: "Hide {item}",
    suggest_collapse: "Show {item} on one line",
    suggest_show: "Show {item} again",
    suggest_rewrite: "Rewrite {item}",
    rewrite_bullet_in: "Rewrite a bullet in {item}",
    edit_then_accept: "Edit then accept",
    edited_text: "Your version",
    needs_attention: "Needs attention",
    badge_new_number: "New number: {detail}",
    badge_new_term: "New term: {detail}",
    badge_terminology: "Terminology changed: {detail}",
    badge_length: "Length: {detail}",
    badge_language: "Language changed: {detail}",

    gaps_heading: "Questions about this job",
    gaps_hint: "The job asks for these and your CV does not clearly show them. Add them only if they are true.",
    gap_missing: "Not in your CV",
    gap_weak: "Only weakly shown",
    gap_must_have: "Must-have",
    gap_nice_to_have: "Nice to have",
    gap_where: "Where it could go: {hint}",
    add_to_cv: "Add to CV…",
    dismiss: "Dismiss",

    ai_review_heading: "Review with AI",
    ai_review_hint: "A review without a job: rewrites and advice for the base CV. Accepting a rewrite changes the base CV.",
    review_cv_with_ai: "Review CV with AI",
    ai_review_progress: "Review progress",
    ai_review_done: "{n} suggestions are ready. Nothing changes until you accept them.",
    ai_review_nothing: "The review found nothing to rewrite.",
    ai_review_cancelled: "The review was cancelled.",
    ai_review_last: "Last review: {at}",
    ai_review_notes: "Advice",
    note_impact: "Impact",
    note_clarity: "Clarity",
    note_structure: "Structure",
    note_consistency: "Consistency",
    whole_cv: "Whole CV",
    wait_for_save: "Wait until your edits are saved.",

    no_secure_random: "Could not add the item: the system's secure random source is unavailable.",
    app_not_answering: "The app did not answer. Restart PinaxCV.",
    unexpected_data: "Unexpected data: {error}",

    error_not_found: "That CV or variant no longer exists. It may have been deleted.",
    error_conflict: "This CV was changed somewhere else, so your latest edits were not saved. Copy anything you need, then reopen the CV.",
    error_cancelled: "Cancelled.",
    error_keychain: "The system keychain could not be used. Check that it is unlocked and try again.",
    error_ai_not_configured: "No AI provider is set up yet. Add one in Settings.",
    error_ai_consent_required: "Allow sending this CV to the AI provider first.",
    error_ai_cancelled: "The AI request was cancelled.",
};

/// The strings for the current locale.
pub fn t() -> &'static Strings {
    &EN
}

/// Replaces each `{name}` in `template` with its value. Unknown placeholders
/// are left as they are, so a missing value shows up instead of vanishing.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after.find('}').and_then(|close| {
            let name = &after[..close];
            args.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v, close))
        });
        match value {
            Some((v, close)) => {
                out.push_str(v);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The text shown for a backend error (spec section 15). Codes whose meaning
/// is complete on its own map to a fixed string here. For the others the
/// backend message is the specific, actionable part (which fields failed
/// validation, what was wrong with an imported file), so it is shown as sent.
pub fn error_message(e: &ApiError) -> String {
    let s = t();
    // No wildcard: a new code must decide here whether it gets its own text.
    let fixed = match e.code {
        ErrorCode::NotFound => Some(s.error_not_found),
        ErrorCode::Conflict => Some(s.error_conflict),
        ErrorCode::Cancelled => Some(s.error_cancelled),
        ErrorCode::Keychain => Some(s.error_keychain),
        ErrorCode::AiNotConfigured => Some(s.error_ai_not_configured),
        ErrorCode::AiConsentRequired => Some(s.error_ai_consent_required),
        ErrorCode::AiCancelled => Some(s.error_ai_cancelled),
        ErrorCode::Validation
        | ErrorCode::Io
        | ErrorCode::Db
        | ErrorCode::Render
        | ErrorCode::Import
        | ErrorCode::UnsupportedFormat
        | ErrorCode::NoTextLayer
        | ErrorCode::AiProvider
        | ErrorCode::AiInvalidOutput
        | ErrorCode::Internal => None,
    };
    match fixed {
        Some(text) => text.to_owned(),
        None => e.message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_named_placeholders() {
        assert_eq!(
            fill(t().exported_to, &[("path", "/tmp/cv.pdf"), ("ats", "")]),
            "Exported to /tmp/cv.pdf"
        );
        assert_eq!(fill("{a}{b}{a}", &[("a", "x"), ("b", "y")]), "xyx");
        // Unknown or unclosed placeholders stay visible.
        assert_eq!(fill("{x} and {", &[]), "{x} and {");
        // Values are not filled again.
        assert_eq!(fill("{a}", &[("a", "{a}")]), "{a}");
        assert_eq!(fill("Variant {n}", &[("n", "é")]), "Variant é");
    }

    #[test]
    fn fixed_codes_use_i18n_text_and_others_keep_the_backend_message() {
        let e = ApiError::new(ErrorCode::NotFound, "backend text");
        assert_eq!(error_message(&e), t().error_not_found);
        let e = ApiError::conflict(3);
        assert_eq!(error_message(&e), t().error_conflict);
        let e = ApiError::new(ErrorCode::Validation, "Add your full name.");
        assert_eq!(error_message(&e), "Add your full name.");
    }

    /// Finds string literals in Rust source outside `#[cfg(test)]` modules
    /// and comments. Good enough for this crate's code, which has no raw
    /// strings or quote characters in char literals.
    fn literals(src: &str) -> Vec<String> {
        let code = src.split("#[cfg(test)]").next().unwrap_or(src);
        let mut out = Vec::new();
        for line in code.lines() {
            let line = line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            let mut chars = line.chars().peekable();
            let mut prev = ' ';
            while let Some(c) = chars.next() {
                if c == '/' && chars.peek() == Some(&'/') {
                    break;
                }
                if c == '"' {
                    let byte_string = prev == 'b';
                    let mut lit = String::new();
                    while let Some(c) = chars.next() {
                        match c {
                            '\\' => {
                                chars.next();
                            }
                            '"' => break,
                            c => lit.push(c),
                        }
                    }
                    if !byte_string {
                        out.push(lit);
                    }
                }
                prev = c;
            }
        }
        out
    }

    /// Text a user could read: a letter outside `{…}` placeholders, and
    /// either a capital letter or a space between words.
    fn looks_like_ui_text(lit: &str) -> bool {
        let mut depth = 0;
        let mut plain = String::new();
        for c in lit.chars() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                c if depth == 0 => plain.push(c),
                _ => {}
            }
        }
        let words = plain.trim();
        words.chars().any(char::is_alphabetic)
            && (words.chars().any(char::is_uppercase) || words.contains(' '))
    }

    /// Every other source file in `src/`.
    const FILES: &[(&str, &str)] = &[
        ("advice.rs", include_str!("advice.rs")),
        ("checks.rs", include_str!("checks.rs")),
        ("dates.rs", include_str!("dates.rs")),
        ("diff.rs", include_str!("diff.rs")),
        ("drag.rs", include_str!("drag.rs")),
        ("editor.rs", include_str!("editor.rs")),
        ("entries.rs", include_str!("entries.rs")),
        ("fit.rs", include_str!("fit.rs")),
        ("history.rs", include_str!("history.rs")),
        ("import.rs", include_str!("import.rs")),
        ("ids.rs", include_str!("ids.rs")),
        ("ipc.rs", include_str!("ipc.rs")),
        ("list.rs", include_str!("list.rs")),
        ("lists.rs", include_str!("lists.rs")),
        ("main.rs", include_str!("main.rs")),
        ("settings.rs", include_str!("settings.rs")),
        ("order.rs", include_str!("order.rs")),
        ("preview.rs", include_str!("preview.rs")),
        ("review.rs", include_str!("review.rs")),
        ("style.rs", include_str!("style.rs")),
        ("tailor.rs", include_str!("tailor.rs")),
        ("text_import.rs", include_str!("text_import.rs")),
        ("variants.rs", include_str!("variants.rs")),
    ];

    #[test]
    fn no_ui_strings_outside_this_module() {
        // The `extern "C"` ABI and identifiers passed to serde or wasm-bindgen.
        const ALLOWED: &[&str] = &[
            "C",
            "camelCase",
            "__TAURI__",
            // `aria-keyshortcuts` values are key names, not prose.
            "Control+Z",
            "Control+Shift+Z",
            "Control+E",
        ];
        let mut found = Vec::new();
        for (name, src) in FILES {
            for lit in literals(src) {
                if looks_like_ui_text(&lit) && !ALLOWED.contains(&lit.as_str()) {
                    found.push(format!("{name}: {lit:?}"));
                }
            }
        }
        assert!(found.is_empty(), "move these into i18n.rs: {found:#?}");
    }

    #[test]
    fn every_module_is_scanned() {
        for line in include_str!("main.rs").lines() {
            let Some(name) = line.strip_prefix("mod ").and_then(|l| l.strip_suffix(';')) else {
                continue;
            };
            let file = format!("{name}.rs");
            assert!(
                name == "i18n" || FILES.iter().any(|(f, _)| *f == file),
                "add {file} to FILES"
            );
        }
    }

    #[test]
    fn literal_scanner_finds_ui_text() {
        let src = "let a = \"Save now\"; // \"Comment\"\nlet b = b\"ABC\"; let c = \"list_cvs\";\n#[cfg(test)]\n\"Test\"";
        assert_eq!(literals(src), ["Save now", "list_cvs"]);
        assert!(looks_like_ui_text("Save now"));
        assert!(looks_like_ui_text("one line"));
        assert!(!looks_like_ui_text("list_cvs"));
        assert!(!looks_like_ui_text("{year}-{m:02}"));
        assert!(!looks_like_ui_text("↑"));
        assert!(!looks_like_ui_text(" "));
    }
}
