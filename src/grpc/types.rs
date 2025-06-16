use std::collections::HashMap;

use crate::{
    types::{SearchRequest as InternalSearchRequest, Filter},
    Document as InternalDocument,
};

use super::*;

/// 从protobuf Document转换为内部Document
impl From<Document> for InternalDocument {
    fn from(proto_doc: Document) -> Self {
        Self {
            id: proto_doc.id,
            content: proto_doc.content,
            title: proto_doc.title,
            language: proto_doc.language,
            package_name: proto_doc.package_name,
            version: proto_doc.version,
            doc_type: proto_doc.doc_type,
            metadata: proto_doc.metadata,
        }
    }
}

/// 从内部Document转换为protobuf Document
impl From<InternalDocument> for Document {
    fn from(internal_doc: InternalDocument) -> Self {
        Self {
            id: internal_doc.id,
            content: internal_doc.content,
            title: internal_doc.title,
            language: internal_doc.language,
            package_name: internal_doc.package_name,
            version: internal_doc.version,
            doc_type: internal_doc.doc_type,
            metadata: internal_doc.metadata,
        }
    }
}

/// 从protobuf SearchDocumentRequest转换为内部SearchRequest
impl TryFrom<SearchDocumentRequest> for InternalSearchRequest {
    type Error = String;

    fn try_from(proto_req: SearchDocumentRequest) -> Result<Self, Self::Error> {
        let filter = if let Some(filter_str) = proto_req.filter {
            // 尝试解析JSON字符串为Filter
            Some(serde_json::from_str::<Filter>(&filter_str)
                .map_err(|e| format!("Invalid filter JSON: {}", e))?)
        } else {
            None
        };

        Ok(Self {
            query: proto_req.query,
            limit: proto_req.limit as usize,
            filter,
        })
    }
} 