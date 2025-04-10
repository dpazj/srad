use crate::payload;

pub struct MetaData {
    pub description: Option<String>,
    pub content_type: Option<String>,
    pub size: Option<u64>,
    pub md5: Option<String>,

    /// File metadata
    pub file_name: Option<String>,
    pub file_type: Option<String>,
}

impl From<MetaData> for payload::MetaData {
    fn from(value: MetaData) -> Self {
        payload::MetaData {
            is_multi_part: None,
            content_type: value.content_type,
            size: value.size,
            seq: None,
            file_name: value.file_name,
            file_type: value.file_type,
            md5: value.md5,
            description: value.description,
        }
    }
}
