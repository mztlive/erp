/** A submitted command has no final result yet; retry the same intent. */
export class SubmissionResultUnknownError extends Error {
    constructor(message: string) {
        super(message)
        this.name = "SubmissionResultUnknownError"
    }
}
